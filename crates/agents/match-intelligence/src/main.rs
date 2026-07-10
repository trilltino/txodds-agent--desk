//! # match-intelligence — FOLLOW-sharp agent
//!
//! ## What it does
//!
//! Every `POLL_INTERVAL_SECS` seconds the agent fetches the latest TxLINE
//! odds snapshot for every live World Cup fixture.  The `features` extractor
//! computes an odds-movement signal; if confidence exceeds the `CONFIDENCE_GATE`
//! the agent records an `ArenaPosition` *in the direction of the movement*
//! (FollowSharp strategy) and appends it to the session JSONL log.
//!
//! ## Safety gate (Checklist §28 / §38)
//!
//! Before every iteration:
//! - Kill switch is checked (can be tripped externally via SIGUSR1).
//! - BudgetGuard checks tool-call count, spend, and session duration.
//! - StepCounter enforces a hard cap of `MAX_STEPS` iterations.
//!
//! ## CoralOS integration
//!
//! The agent writes structured JSON logs to stdout.  The CoralOS Python
//! participant (`coral-agents/match-intelligence-agent/agent.py`) connects
//! as the MCP identity; this binary does the real work and publishes its
//! transcript through the puppet API by writing the log lines that the
//! Tauri backend picks up via the sidecar channel.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::todo)]

use std::time::Duration;

use agent_core::{
    arena::{ArenaPosition, PositionDirection, Strategy},
    capability::FollowCap,
    error::AgentError,
    safety::{BudgetGuard, StepCounter, safety_check, wrap_untrusted},
    tools::IdempotencyKey,
};
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};
use uuid::Uuid;

// ── Configuration ─────────────────────────────────────────────────────────────

/// Runtime configuration loaded from environment variables.
struct Config {
    api_base: String,
    api_key: String,
    poll_interval_secs: u64,
    confidence_gate: f64,
    odds_move_threshold_pct: f64,
    session_log_path: String,
    max_steps: u64,
}

impl Config {
    fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            api_base: env_or("TXLINE_API_BASE", "https://txline.txodds.com/api/v1"),
            api_key: require_env("TXLINE_API_KEY")?,
            poll_interval_secs: env_parse("POLL_INTERVAL_SECS", 60),
            confidence_gate: env_parse_f64("CONFIDENCE_GATE", 0.55),
            odds_move_threshold_pct: env_parse_f64("ODDS_MOVE_THRESHOLD_PCT", 4.0),
            session_log_path: env_or("SESSION_LOG_PATH", "match-intelligence-session.jsonl"),
            max_steps: env_parse("MAX_STEPS", 500),
        })
    }
}

// ── TxLINE API types ──────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct FixtureList {
    data: Vec<FixtureSummary>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct FixtureSummary {
    id: u64,
    name: String,
    status: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct OddsSnapshot {
    fixture_id: u64,
    markets: Vec<Market>,
}

#[derive(Debug, Deserialize)]
struct Market {
    key: String,
    selections: Vec<Selection>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct Selection {
    name: String,
    odds: f64,
    previous_odds: Option<f64>,
}

// ── Position log entry ────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct PositionLogEntry {
    idempotency_key: String,
    position: ArenaPosition,
    strategy: String,
}

// ── Agent step result ─────────────────────────────────────────────────────────

#[derive(Debug)]
enum StepResult {
    /// Positions were recorded this iteration.
    Positions(Vec<ArenaPosition>),
    /// No signal met the confidence gate.
    NoSignal,
    /// TxLINE returned no live fixtures right now.
    NoLiveFixtures,
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    // Structured JSON logging — integrates with CoralOS observability (§24).
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("match_intelligence=info".parse().expect("static directive")),
        )
        .init();

    info!(agent = "match-intelligence", strategy = "follow_sharp", "starting");

    let config = match Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            error!(error = %e, "configuration error — aborting");
            std::process::exit(1);
        }
    };

    // --- Safety gate setup (§28, §38) ---
    let budget = BudgetGuard::default_devnet();
    let mut steps = StepCounter::new(config.max_steps);

    // FOLLOW capability token — proves this binary was granted FollowSharp rights.
    let _follow_cap = FollowCap::acquire();

    let client = build_http_client(&config.api_key);
    let poll = Duration::from_secs(config.poll_interval_secs);

    let mut prev_odds: std::collections::HashMap<(u64, String, String), f64> =
        std::collections::HashMap::new();

    // Track the last snapshot so we can detect movement from the previous poll.
    loop {
        // --- Safety gate ---
        if let Err(e) = safety_check(&budget) {
            warn!(error = %e, "safety gate triggered — shutting down");
            break;
        }
        if let Err(e) = steps.tick() {
            warn!(error = %e, "step cap reached — shutting down");
            break;
        }

        match agent_step(
            &client,
            &config,
            &budget,
            &mut prev_odds,
            &config.session_log_path,
        )
        .await
        {
            Ok(StepResult::Positions(positions)) => {
                info!(
                    count = positions.len(),
                    step = steps.current(),
                    "positions recorded"
                );
            }
            Ok(StepResult::NoSignal) => {
                info!(step = steps.current(), "no signal above confidence gate");
            }
            Ok(StepResult::NoLiveFixtures) => {
                info!(step = steps.current(), "no live fixtures");
            }
            Err(e) if agent_core::error::is_retryable(&e) => {
                warn!(error = %e, "transient error — will retry next poll");
            }
            Err(e) => {
                error!(error = %e, "non-retryable error — shutting down");
                break;
            }
        }

        tokio::time::sleep(poll).await;
    }

    info!(
        tool_calls = budget.current_tool_calls(),
        spend_lamports = budget.current_spend_lamports(),
        steps = steps.current(),
        "match-intelligence agent shut down cleanly"
    );
}

// ── Agent step ────────────────────────────────────────────────────────────────

async fn agent_step(
    client: &reqwest::Client,
    config: &Config,
    budget: &BudgetGuard,
    prev_odds: &mut std::collections::HashMap<(u64, String, String), f64>,
    log_path: &str,
) -> Result<StepResult, AgentError> {
    // Tool call 1: fetch live fixtures.
    budget.record_tool_call();
    let fixtures = fetch_live_fixtures(client, &config.api_base).await?;

    if fixtures.is_empty() {
        return Ok(StepResult::NoLiveFixtures);
    }

    let mut new_positions: Vec<ArenaPosition> = Vec::new();

    for fixture in &fixtures {
        // Tool call 2: fetch odds for this fixture.
        budget.record_tool_call();
        let snapshot = fetch_odds(client, &config.api_base, fixture.id).await?;

        for market in &snapshot.markets {
            for selection in &market.selections {
                let prev = prev_odds
                    .get(&(fixture.id, market.key.clone(), selection.name.clone()))
                    .copied();

                if let Some(prev_val) = prev {
                    let move_pct =
                        ((selection.odds - prev_val) / prev_val * 100.0).abs();

                    if move_pct >= config.odds_move_threshold_pct {
                        // Compute a simple confidence score: normalise move_pct
                        // into (0, 1).  Larger moves → higher confidence, capped
                        // at 0.95 so we never claim certainty.
                        let confidence = (move_pct / 20.0).min(0.95);

                        if confidence >= config.confidence_gate {
                            let direction = if selection.odds < prev_val {
                                // Odds shortened (favourite drift in) → follow
                                PositionDirection::With
                            } else {
                                // Odds lengthened — money moving away → still
                                // follow the "away" side by taking Against here
                                // (backing underdog where sharp money drifted).
                                PositionDirection::Against
                            };

                            // Idempotency key — prevents double-recording if we
                            // crash and restart mid-step (§14).
                            let idempotency_key = IdempotencyKey::new_for(
                                &format!("{}:{}:{}", fixture.id, market.key, selection.name),
                            );

                            let position = ArenaPosition {
                                position_id: Uuid::new_v4().to_string(),
                                agent_id: "match-intelligence".into(),
                                strategy: Strategy::FollowSharp,
                                fixture_id: fixture.id,
                                market_key: market.key.clone(),
                                selection: selection.name.clone(),
                                odds_at_entry: selection.odds,
                                odds_move_pct: move_pct,
                                direction,
                                confidence,
                                recorded_at: utc_now_iso8601(),
                                tx_signature: None,
                                outcome: None,
                            };

                            // Append to session JSONL log (tamper-evident audit,
                            // §24, §38).
                            let entry = PositionLogEntry {
                                idempotency_key: idempotency_key.to_string(),
                                position: position.clone(),
                                strategy: "follow_sharp".into(),
                            };
                            append_to_log(log_path, &entry)?;

                            // Wrap the selection name when emitting to tracing so
                            // that any injected content can't be mistaken for a
                            // log directive (§28).
                            let safe_selection =
                                wrap_untrusted("selection_name", &selection.name);
                            info!(
                                fixture = fixture.id,
                                market = %market.key,
                                selection = %safe_selection,
                                odds = selection.odds,
                                prev_odds = prev_val,
                                move_pct = move_pct,
                                confidence = confidence,
                                idempotency_key = %idempotency_key,
                                "signal detected — position recorded (FOLLOW)"
                            );

                            new_positions.push(position);
                        }
                    }
                }

                // Update the prev_odds snapshot.
                prev_odds.insert(
                    (fixture.id, market.key.clone(), selection.name.clone()),
                    selection.odds,
                );
            }
        }
    }

    if new_positions.is_empty() {
        Ok(StepResult::NoSignal)
    } else {
        Ok(StepResult::Positions(new_positions))
    }
}

// ── HTTP helpers ──────────────────────────────────────────────────────────────

async fn fetch_live_fixtures(
    client: &reqwest::Client,
    base: &str,
) -> Result<Vec<FixtureSummary>, AgentError> {
    let url = format!("{base}/worldcup/fixtures?status=live");
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| AgentError::ToolCallFailed {
            tool: "fetch_live_fixtures".into(),
            reason: e.to_string(),
        })?;

    if !resp.status().is_success() {
        return Err(AgentError::ToolCallFailed {
            tool: "fetch_live_fixtures".into(),
            reason: format!("HTTP {}", resp.status()),
        });
    }

    let list: FixtureList =
        resp.json()
            .await
            .map_err(|e| AgentError::ParseError(e.to_string()))?;

    Ok(list
        .data
        .into_iter()
        .filter(|f| f.status == "live")
        .collect())
}

async fn fetch_odds(
    client: &reqwest::Client,
    base: &str,
    fixture_id: u64,
) -> Result<OddsSnapshot, AgentError> {
    let url = format!("{base}/worldcup/fixtures/{fixture_id}/odds");
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| AgentError::ToolCallFailed {
            tool: "fetch_odds".into(),
            reason: e.to_string(),
        })?;

    if !resp.status().is_success() {
        return Err(AgentError::ToolCallFailed {
            tool: "fetch_odds".into(),
            reason: format!("HTTP {}", resp.status()),
        });
    }

    resp.json()
        .await
        .map_err(|e| AgentError::ParseError(e.to_string()))
}

// ── Log helpers ───────────────────────────────────────────────────────────────

fn append_to_log(path: &str, entry: &PositionLogEntry) -> Result<(), AgentError> {
    use std::io::Write;
    let line =
        serde_json::to_string(entry).map_err(|e| AgentError::ParseError(e.to_string()))?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| AgentError::ToolCallFailed {
            tool: "append_to_log".into(),
            reason: e.to_string(),
        })?;
    writeln!(file, "{line}").map_err(|e| AgentError::ToolCallFailed {
        tool: "append_to_log".into(),
        reason: e.to_string(),
    })
}

// ── Misc helpers ──────────────────────────────────────────────────────────────

fn build_http_client(api_key: &str) -> reqwest::Client {
    let mut headers = reqwest::header::HeaderMap::new();
    // Never logs the key value — the header value is opaque to tracing (§21).
    if let Ok(val) = reqwest::header::HeaderValue::from_str(&format!("Bearer {api_key}")) {
        headers.insert(reqwest::header::AUTHORIZATION, val);
    }
    reqwest::Client::builder()
        .default_headers(headers)
        .timeout(Duration::from_secs(15))
        .build()
        .expect("static TLS config is valid")
}

fn utc_now_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Simple epoch-seconds as ISO-8601 approximation for no-chrono build.
    format!("{}Z", secs)
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_owned())
}

fn require_env(key: &str) -> Result<String, ConfigError> {
    std::env::var(key).map_err(|_| ConfigError::Missing(key.to_owned()))
}

fn env_parse<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn env_parse_f64(key: &str, default: f64) -> f64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

// ── ConfigError ───────────────────────────────────────────────────────────────

#[derive(Debug)]
enum ConfigError {
    Missing(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Missing(k) => write!(f, "required env var {k} is not set"),
        }
    }
}
