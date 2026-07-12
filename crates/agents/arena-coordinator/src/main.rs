//! # arena-coordinator — settlement authority
//!
//! Polls both agent JSONL logs and TxLINE fixture results.  When a fixture
//! completes it settles all open positions by comparing each position's
//! selection against the actual result, writes a `SettlementRecord` to the
//! shared settlement log, and logs the winner of the FollowSharp vs FadeSharp
//! strategy contest.  The `SettleCap` is the only capability token that can
//! call the `settle_positions` function — §8.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::todo)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use agent_core::{
    arena::{ArenaPosition, PositionDirection, Strategy},
    capability::SettleCap,
    error::AgentError,
    safety::{BudgetGuard, StepCounter, safety_check},
    tools::IdempotencyKey,
};
use coral_client::{wire, CoralMention, Specialist};
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

// ── Config ────────────────────────────────────────────────────────────────────

struct Config {
    api_base: String,
    api_key: String,
    poll_secs: u64,
    follow_log: String,
    fade_log: String,
    settlement_log: String,
    max_steps: u64,
}

impl Config {
    fn from_env() -> Result<Self, String> {
        Ok(Self {
            api_base: env_or("TXLINE_API_BASE", "https://txline.txodds.com/api/v1"),
            api_key: std::env::var("TXLINE_API_KEY")
                .map_err(|_| "TXLINE_API_KEY not set".to_owned())?,
            poll_secs: env_parse("COORDINATOR_POLL_SECS", 30),
            follow_log: env_or("FOLLOW_LOG", "match-intelligence-session.jsonl"),
            fade_log: env_or("FADE_LOG", "contrarian-session.jsonl"),
            settlement_log: env_or("SETTLEMENT_LOG", "arena-settlement.jsonl"),
            max_steps: env_parse("MAX_STEPS", 1000),
        })
    }
}

// ── TxLINE result types ───────────────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct FixtureResult {
    fixture_id: u64,
    status: String,
    result: Option<FixtureOutcome>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct FixtureOutcome {
    winner: String,
    home_score: u32,
    away_score: u32,
}

#[derive(Debug, Deserialize)]
struct FixtureListResp {
    data: Vec<FixtureSummary>,
}

#[derive(Debug, Deserialize)]
struct FixtureSummary {
    id: u64,
    status: String,
}

// ── Settlement types ──────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Clone)]
struct SettlementRecord {
    idempotency_key: String,
    fixture_id: u64,
    agent_id: String,
    strategy: String,
    market_key: String,
    selection: String,
    direction: String,
    odds_at_entry: f64,
    result: String,
    pnl_units: f64,
    settled_at: String,
}

// ── Arena score ───────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
struct ArenaScore {
    follow_wins: u32,
    follow_losses: u32,
    fade_wins: u32,
    fade_losses: u32,
}

impl ArenaScore {
    fn follow_pnl(&self) -> i64 {
        self.follow_wins as i64 - self.follow_losses as i64
    }
    fn fade_pnl(&self) -> i64 {
        self.fade_wins as i64 - self.fade_losses as i64
    }
    fn leader(&self) -> &'static str {
        match self.follow_pnl().cmp(&self.fade_pnl()) {
            std::cmp::Ordering::Greater => "FOLLOW (match-intelligence)",
            std::cmp::Ordering::Less => "FADE (contrarian)",
            std::cmp::Ordering::Equal => "TIE",
        }
    }
}

// ── Coral bus participation (hybrid mode, TODO 6c) ────────────────────────────

/// Read-only snapshot of the arena scoreboard for the Coral mention loop.
/// The poll loop's `ArenaScore` stays authoritative; `publish_status` copies
/// it here after every successful coordinator step.
#[derive(Default)]
struct ArenaStatusShared {
    settled: AtomicU64,
    follow_wins: AtomicU64,
    follow_losses: AtomicU64,
    fade_wins: AtomicU64,
    fade_losses: AtomicU64,
}

fn publish_status(shared: &ArenaStatusShared, score: &ArenaScore, settled_total: u64) {
    shared.settled.store(settled_total, Ordering::Relaxed);
    shared.follow_wins.store(u64::from(score.follow_wins), Ordering::Relaxed);
    shared.follow_losses.store(u64::from(score.follow_losses), Ordering::Relaxed);
    shared.fade_wins.store(u64::from(score.fade_wins), Ordering::Relaxed);
    shared.fade_losses.store(u64::from(score.fade_losses), Ordering::Relaxed);
}

/// Hybrid-mode Specialist: when coral-server spawned this process
/// (`CORAL_CONNECTION_URL` present), the settlement poll loop keeps running
/// unchanged and this answers `ARENA_STATUS_REQUESTED` mentions with the
/// live scoreboard. Same transport constraint and rationale as contrarian's
/// `ContrarianStatusSpecialist` — mention-driven status is the honest
/// hybrid (TODO 6c option b).
struct ArenaStatusSpecialist {
    shared: Arc<ArenaStatusShared>,
}

#[async_trait::async_trait]
impl Specialist for ArenaStatusSpecialist {
    fn name(&self) -> &str {
        "arena-coordinator"
    }

    async fn handle(&self, mention: CoralMention) -> String {
        if wire::verb(&mention.text) != "ARENA_STATUS_REQUESTED" {
            tracing::debug!(text = %mention.text, "arena-coordinator: ignoring non-status mention");
            return String::new();
        }
        let settled = self.shared.settled.load(Ordering::Relaxed);
        let fw = self.shared.follow_wins.load(Ordering::Relaxed);
        let fl = self.shared.follow_losses.load(Ordering::Relaxed);
        let dw = self.shared.fade_wins.load(Ordering::Relaxed);
        let dl = self.shared.fade_losses.load(Ordering::Relaxed);
        let leader = match (fw as i64 - fl as i64).cmp(&(dw as i64 - dl as i64)) {
            std::cmp::Ordering::Greater => "FOLLOW",
            std::cmp::Ordering::Less => "FADE",
            std::cmp::Ordering::Equal => "TIE",
        };
        format!(
            "ARENA_STATUS settled={settled} followWins={fw} followLosses={fl} fadeWins={dw} fadeLosses={dl} leader={leader}"
        )
    }
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("arena_coordinator=info".parse().expect("static")),
        )
        .init();

    info!(agent = "arena-coordinator", "starting");

    let config = match Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            error!(error = %e, "config error — aborting");
            std::process::exit(1);
        }
    };

    // Only the coordinator holds SettleCap (§8).
    let _settle_cap = SettleCap::acquire();

    let budget = BudgetGuard::default_devnet();
    let mut steps = StepCounter::new(config.max_steps);

    let client = build_http_client(&config.api_key);
    let poll = Duration::from_secs(config.poll_secs);

    // Track which fixture+position combos we've already settled (idempotency).
    let mut settled: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut score = ArenaScore::default();

    // Hybrid mode (TODO 6c): when coral-server spawned this process it also
    // participates on the Coral bus, answering ARENA_STATUS_REQUESTED with
    // the scoreboard snapshot published after each step. The poll loop is
    // unchanged and stays authoritative.
    let shared = Arc::new(ArenaStatusShared::default());
    if std::env::var("CORAL_CONNECTION_URL").is_ok() {
        let specialist = ArenaStatusSpecialist { shared: Arc::clone(&shared) };
        let coral_wait_ms: u64 = env_parse("CORAL_MAX_WAIT_MS", 30_000);
        let coral_steps: u64 = env_parse("CORAL_MAX_STEPS", 100_000);
        tokio::spawn(async move {
            if let Err(e) = coral_client::run(specialist, coral_wait_ms, coral_steps).await {
                warn!(error = %e, "coral mention loop ended");
            }
        });
        info!("hybrid mode: servicing ARENA_STATUS_REQUESTED on the Coral bus");
    } else {
        info!("standalone mode: no CORAL_CONNECTION_URL, poll loop only");
    }

    loop {
        if let Err(e) = safety_check(&budget) {
            warn!(error = %e, "safety gate — shutting down");
            break;
        }
        if let Err(e) = steps.tick() {
            warn!(error = %e, "step cap — shutting down");
            break;
        }

        match coordinator_step(
            &client,
            &config,
            &budget,
            &mut settled,
            &mut score,
        )
        .await
        {
            Ok(n) => {
                if n > 0 {
                    info!(settled = n, step = steps.current(), leader = score.leader(), "positions settled");
                } else {
                    info!(step = steps.current(), "no new settlements");
                }
                publish_status(&shared, &score, settled.len() as u64);
            }
            Err(e) if agent_core::error::is_retryable(&e) => warn!(error=%e, "transient — retrying"),
            Err(e) => { error!(error=%e, "fatal — shutting down"); break; }
        }

        tokio::time::sleep(poll).await;
    }

    info!(
        follow_pnl = score.follow_pnl(),
        fade_pnl = score.fade_pnl(),
        leader = score.leader(),
        "arena-coordinator shut down"
    );
}

// ── Coordinator step ──────────────────────────────────────────────────────────

async fn coordinator_step(
    client: &reqwest::Client,
    config: &Config,
    budget: &BudgetGuard,
    settled: &mut std::collections::HashSet<String>,
    score: &mut ArenaScore,
) -> Result<usize, AgentError> {
    // 1. Fetch completed fixtures from TxLINE.
    budget.record_tool_call();
    let completed = fetch_completed_fixtures(client, &config.api_base).await?;
    if completed.is_empty() {
        return Ok(0);
    }

    // 2. Load open positions from both agent logs.
    let follow_positions = load_positions(&config.follow_log);
    let fade_positions = load_positions(&config.fade_log);
    let all_positions: Vec<ArenaPosition> = follow_positions
        .into_iter()
        .chain(fade_positions)
        .collect();

    // Build a map: fixture_id → result.
    let results: HashMap<u64, &FixtureResult> =
        completed.iter().map(|r| (r.fixture_id, r)).collect();

    let mut n_settled = 0usize;

    for position in &all_positions {
        // Skip already-settled positions (idempotency — §14).
        if settled.contains(&position.position_id) {
            continue;
        }

        let Some(fixture_result) = results.get(&position.fixture_id) else {
            continue; // Not yet complete.
        };

        let Some(outcome) = &fixture_result.result else {
            continue;
        };

        // Determine win/loss: does the selection match the actual winner?
        let selection_won = outcome.winner.to_lowercase() == position.selection.to_lowercase()
            || outcome.winner.to_lowercase().contains(&position.selection.to_lowercase());

        let won = match position.direction {
            PositionDirection::With => selection_won,
            PositionDirection::Against => !selection_won,
        };

        let pnl_units = if won { position.odds_at_entry - 1.0 } else { -1.0 };

        let record = SettlementRecord {
            idempotency_key: IdempotencyKey::new_for(
                &format!("settle:{}", position.position_id)
            ).to_string(),
            fixture_id: position.fixture_id,
            agent_id: position.agent_id.clone(),
            strategy: format!("{:?}", position.strategy),
            market_key: position.market_key.clone(),
            selection: position.selection.clone(),
            direction: format!("{:?}", position.direction),
            odds_at_entry: position.odds_at_entry,
            result: if won { "win".into() } else { "loss".into() },
            pnl_units,
            settled_at: utc_now(),
        };

        append_to_log(&config.settlement_log, &record)?;
        settled.insert(position.position_id.clone());

        // Update arena score.
        match position.strategy {
            Strategy::FollowSharp => {
                if won { score.follow_wins += 1; } else { score.follow_losses += 1; }
            }
            Strategy::FadeSharp => {
                if won { score.fade_wins += 1; } else { score.fade_losses += 1; }
            }
        }

        info!(
            fixture = position.fixture_id,
            agent = %position.agent_id,
            selection = %position.selection,
            won,
            pnl = pnl_units,
            leader = score.leader(),
            "settled"
        );

        n_settled += 1;
    }

    Ok(n_settled)
}

// ── HTTP helpers ──────────────────────────────────────────────────────────────

async fn fetch_completed_fixtures(
    client: &reqwest::Client,
    base: &str,
) -> Result<Vec<FixtureResult>, AgentError> {
    let url = format!("{base}/worldcup/fixtures?status=finished");
    let resp = client.get(&url).send().await.map_err(|e| AgentError::ToolCallFailed {
        tool: "fetch_completed_fixtures".into(),
        reason: e.to_string(),
    })?;
    if !resp.status().is_success() {
        return Err(AgentError::ToolCallFailed {
            tool: "fetch_completed_fixtures".into(),
            reason: format!("HTTP {}", resp.status()),
        });
    }
    let list: FixtureListResp = resp.json().await
        .map_err(|e| AgentError::ParseError(e.to_string()))?;

    let mut results = Vec::new();
    for f in list.data.iter().filter(|f| f.status == "finished") {
        let detail_url = format!("{base}/worldcup/fixtures/{}", f.id);
        if let Ok(r) = client.get(&detail_url).send().await {
            if let Ok(fixture_result) = r.json::<FixtureResult>().await {
                results.push(fixture_result);
            }
        }
    }
    Ok(results)
}

// ── Log helpers ───────────────────────────────────────────────────────────────

fn load_positions(path: &str) -> Vec<ArenaPosition> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    content
        .lines()
        .filter_map(|line| {
            #[derive(Deserialize)]
            struct Entry { position: ArenaPosition }
            serde_json::from_str::<Entry>(line).ok().map(|e| e.position)
        })
        .collect()
}

fn append_to_log(path: &str, record: &SettlementRecord) -> Result<(), AgentError> {
    use std::io::Write;
    let line = serde_json::to_string(record)
        .map_err(|e| AgentError::ParseError(e.to_string()))?;
    let mut f = std::fs::OpenOptions::new()
        .create(true).append(true).open(path)
        .map_err(|e| AgentError::ToolCallFailed { tool: "append_to_log".into(), reason: e.to_string() })?;
    writeln!(f, "{line}").map_err(|e| AgentError::ToolCallFailed {
        tool: "append_to_log".into(), reason: e.to_string(),
    })
}

// ── Misc ──────────────────────────────────────────────────────────────────────

fn build_http_client(api_key: &str) -> reqwest::Client {
    let mut headers = reqwest::header::HeaderMap::new();
    if let Ok(v) = reqwest::header::HeaderValue::from_str(&format!("Bearer {api_key}")) {
        headers.insert(reqwest::header::AUTHORIZATION, v);
    }
    reqwest::Client::builder()
        .default_headers(headers)
        .timeout(Duration::from_secs(15))
        .build()
        .expect("static TLS config")
}

fn utc_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let s = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    format!("{s}Z")
}

fn env_or(k: &str, default: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| default.to_owned())
}

fn env_parse<T: std::str::FromStr>(k: &str, default: T) -> T {
    std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn mention(text: &str) -> CoralMention {
        CoralMention {
            thread_id: Some("t-1".into()),
            sender: Some("match-intelligence-agent".into()),
            text: text.to_owned(),
        }
    }

    #[tokio::test]
    async fn status_specialist_reports_scoreboard() {
        let shared = Arc::new(ArenaStatusShared::default());
        let score = ArenaScore {
            follow_wins: 3,
            follow_losses: 1,
            fade_wins: 1,
            fade_losses: 2,
        };
        publish_status(&shared, &score, 7);
        let specialist = ArenaStatusSpecialist { shared };
        let reply = specialist.handle(mention("ARENA_STATUS_REQUESTED")).await;
        assert_eq!(
            reply,
            "ARENA_STATUS settled=7 followWins=3 followLosses=1 fadeWins=1 fadeLosses=2 leader=FOLLOW"
        );
    }

    #[tokio::test]
    async fn status_specialist_reports_tie_before_any_settlement() {
        let specialist = ArenaStatusSpecialist { shared: Arc::new(ArenaStatusShared::default()) };
        let reply = specialist.handle(mention("ARENA_STATUS_REQUESTED")).await;
        assert_eq!(
            reply,
            "ARENA_STATUS settled=0 followWins=0 followLosses=0 fadeWins=0 fadeLosses=0 leader=TIE"
        );
    }

    #[tokio::test]
    async fn status_specialist_ignores_other_verbs() {
        let specialist = ArenaStatusSpecialist { shared: Arc::new(ArenaStatusShared::default()) };
        assert_eq!(specialist.handle(mention("SETTLE_REQUESTED wager={}")).await, "");
    }
}

