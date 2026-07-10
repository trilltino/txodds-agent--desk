//! # contrarian — FADE-sharp agent
//!
//! ## What it does
//!
//! Reads the exact same TxLINE feed as `match-intelligence` but bets
//! *against* every sharp-movement signal (FadeSharp strategy).  Positions
//! are recorded to a separate JSONL log and settled by `arena-coordinator`.
//!
//! ## Safety gate (§28, §38)
//!
//! Identical gate to match-intelligence: kill switch, BudgetGuard,
//! StepCounter.  The only behavioural difference is the `FadeCap` capability
//! token and the inverted `PositionDirection`.
//!
//! ## Why two separate binaries?
//!
//! Running two independent OS processes with separate capability tokens means
//! a misbehaving contrarian agent cannot escalate to FollowSharp rights, and
//! vice versa.  The arena-coordinator is the only process that holds
//! `SettleCap`.  This is the "capability-based security" pattern from §8.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::todo)]

use std::collections::HashMap;
use std::time::Duration;

use agent_core::{
    arena::{ArenaPosition, PositionDirection, Strategy},
    capability::FadeCap,
    error::AgentError,
    safety::{BudgetGuard, StepCounter, safety_check, wrap_untrusted},
    tools::IdempotencyKey,
};
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};
use uuid::Uuid;

// ── Configuration ─────────────────────────────────────────────────────────────

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
            session_log_path: env_or("SESSION_LOG_PATH", "contrarian-session.jsonl"),
            max_steps: env_parse("MAX_STEPS", 500),
        })
    }
}

// ── TxLINE API types ──────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct FixtureList {
    data: Vec<FixtureSummary>,
}

#[derive(Debug, Deserialize)]
struct FixtureSummary {
    id: u64,
    #[allow(dead_code)]
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

// ── Log entry ─────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct PositionLogEntry {
    idempotency_key: String,
    position: ArenaPosition,
    strategy: String,
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("contrarian=info".parse().expect("static directive")),
        )
        .init();

    info!(agent = "contrarian", strategy = "fade_sharp", "starting");

    let config = match Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            error!(error = %e, "configuration error — aborting");
            std::process::exit(1);
        }
    };

    let budget = BudgetGuard::default_devnet();
    let mut steps = StepCounter::new(config.max_steps);

    // FADE capability token — proves this process holds FadeSharp rights (§8).
    let _fade_cap = FadeCap::acquire();

    let client = build_http_client(&config.api_key);
    let poll = Duration::from_secs(config.poll_interval_secs);
    let mut prev_odds: HashMap<(u64, String, String), f64> = HashMap::new();

    loop {
        if let Err(e) = safety_check(&budget) {
            warn!(error = %e, "safety gate triggered — shutting down");
            break;
        }
        if let Err(e) = steps.tick() {
            warn!(error = %e, "step cap reached — shutting down");
            break;
        }

        match agent_step(&client, &config, &budget, &mut prev_odds, &config.session_log_path)
            .await
        {
            Ok(count) if count > 0 => {
                info!(count, step = steps.current(), "positions recorded (FADE)");
            }
            Ok(_) => {
                info!(step = steps.current(), "no signal above confidence gate");
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
        steps = steps.current(),
        "contrarian agent shut down cleanly"
    );
}

// ── Agent step ────────────────────────────────────────────────────────────────

async fn agent_step(
    client: &reqwest::Client,
    config: &Config,
    budget: &BudgetGuard,
    prev_odds: &mut HashMap<(u64, String, String), f64>,
    log_path: &str,
) -> Result<usize, AgentError> {
    budget.record_tool_call();
    let fixtures = fetch_live_fixtures(client, &config.api_base).await?;
    if fixtures.is_empty() {
        return Ok(0);
    }

    let mut recorded = 0usize;

    for fixture in &fixtures {
        budget.record_tool_call();
        let snapshot = fetch_odds(client, &config.api_base, fixture.id).await?;

        for market in &snapshot.markets {
            for sel in &market.selections {
                let key = (fixture.id, market.key.clone(), sel.name.clone());
                let prev = prev_odds.get(&key).copied();

                if let Some(prev_val) = prev {
                    let move_pct = ((sel.odds - prev_val) / prev_val * 100.0).abs();

                    if move_pct >= config.odds_move_threshold_pct {
                        let confidence = (move_pct / 20.0).min(0.95);

                        if confidence >= config.confidence_gate {
                            // FADE: invert the direction that match-intelligence
                            // would take.  If odds shortened (sharp money in),
                            // contrarian bets Against the favourite.
                            let direction = if sel.odds < prev_val {
                                PositionDirection::Against
                            } else {
                                PositionDirection::With
                            };

                            let idempotency_key = IdempotencyKey::new_for(&format!(
                                "fade:{}:{}:{}",
                                fixture.id, market.key, sel.name
                            ));

                            let position = ArenaPosition {
                                position_id: Uuid::new_v4().to_string(),
                                agent_id: "contrarian".into(),
                                strategy: Strategy::FadeSharp,
                                fixture_id: fixture.id,
                                market_key: market.key.clone(),
                                selection: sel.name.clone(),
                                odds_at_entry: sel.odds,
                                odds_move_pct: move_pct,
                                direction,
                                confidence,
                                recorded_at: utc_now_iso8601(),
                                tx_signature: None,
                                outcome: None,
                            };

                            append_to_log(
                                log_path,
                                &PositionLogEntry {
                                    idempotency_key: idempotency_key.to_string(),
                                    position: position.clone(),
                                    strategy: "fade_sharp".into(),
                                },
                            )?;

                            let safe_sel = wrap_untrusted("selection_name", &sel.name);
                            info!(
                                fixture = fixture.id,
                                market = %market.key,
                                selection = %safe_sel,
                                odds = sel.odds,
                                prev_odds = prev_val,
                                move_pct,
                                confidence,
                                idempotency_key = %idempotency_key,
                                "signal detected — position recorded (FADE)"
                            );

                            recorded += 1;
                        }
                    }
                }

                prev_odds.insert(key, sel.odds);
            }
        }
    }

    Ok(recorded)
}

// ── HTTP helpers (identical to match-intelligence) ────────────────────────────

async fn fetch_live_fixtures(
    client: &reqwest::Client,
    base: &str,
) -> Result<Vec<FixtureSummary>, AgentError> {
    let url = format!("{base}/worldcup/fixtures?status=live");
    let resp = client.get(&url).send().await.map_err(|e| AgentError::ToolCallFailed {
        tool: "fetch_live_fixtures".into(),
        reason: e.to_string(),
    })?;
    if !resp.status().is_success() {
        return Err(AgentError::ToolCallFailed {
            tool: "fetch_live_fixtures".into(),
            reason: format!("HTTP {}", resp.status()),
        });
    }
    let list: FixtureList = resp.json().await.map_err(|e| AgentError::ParseError(e.to_string()))?;
    Ok(list.data.into_iter().filter(|f| f.status == "live").collect())
}

async fn fetch_odds(
    client: &reqwest::Client,
    base: &str,
    fixture_id: u64,
) -> Result<OddsSnapshot, AgentError> {
    let url = format!("{base}/worldcup/fixtures/{fixture_id}/odds");
    let resp = client.get(&url).send().await.map_err(|e| AgentError::ToolCallFailed {
        tool: "fetch_odds".into(),
        reason: e.to_string(),
    })?;
    if !resp.status().is_success() {
        return Err(AgentError::ToolCallFailed {
            tool: "fetch_odds".into(),
            reason: format!("HTTP {}", resp.status()),
        });
    }
    resp.json().await.map_err(|e| AgentError::ParseError(e.to_string()))
}

fn append_to_log(path: &str, entry: &PositionLogEntry) -> Result<(), AgentError> {
    use std::io::Write;
    let line = serde_json::to_string(entry).map_err(|e| AgentError::ParseError(e.to_string()))?;
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

fn build_http_client(api_key: &str) -> reqwest::Client {
    let mut headers = reqwest::header::HeaderMap::new();
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
    format!("{}Z", secs)
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_owned())
}

fn require_env(key: &str) -> Result<String, ConfigError> {
    std::env::var(key).map_err(|_| ConfigError::Missing(key.to_owned()))
}

fn env_parse<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn env_parse_f64(key: &str, default: f64) -> f64 {
    std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

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
