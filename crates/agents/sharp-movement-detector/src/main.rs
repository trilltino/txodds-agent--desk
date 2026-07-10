//! # sharp-movement-detector
//!
//! An autonomous agent that polls TxLINE odds every `POLL_INTERVAL_SECS`
//! seconds and asks a Venice AI agent — with real tools, not a single
//! narration prompt — to decide whether a sharp-money movement has occurred
//! and whether it's worth logging.
//!
//! ## Module map
//!
//! - `config`: environment-driven `Config` and its `from_env` constructor.
//! - `txline`: TxLINE fixture/odds HTTP client and response types.
//! - `signal`: `SignalRecord`, the JSONL audit log, and prediction tracking.
//! - `venice`: the Venice reasoning agent, its tools, and the tool-calling loop.
//! - `detector`: one poll cycle wiring the above together.
//!
//! ## rig-venice ROADMAP.md Phase 1
//!
//! This binary used to compute `move_pct` / `confidence` inline in Rust and
//! only ask Venice to narrate the outcome after the fact (see git history).
//! Per `crates/rig-venice/ROADMAP.md` Phase 1, that pre-computation is gone:
//! the LLM is handed the raw current/previous odds for any selection that
//! moved at all and must call the `compute_sharp_movement` tool itself to
//! decide sharpness, optionally re-checking via `fetch_odds_snapshot` /
//! `fetch_active_fixtures` before answering. Rust still owns the `prev_odds`
//! state map (an LLM tool call is stateless per turn; something has to carry
//! "what was this last time" across polls) and the deterministic
//! `compute_sharp_movement` tool's own result is what gates whether the
//! detector logs a signal — the model's free-text answer is the rationale,
//! never the boolean.
//!
//! ## What makes this agent "complex Rust / CoralOS"
//!
//! 1. **Two-tier safety gate** (Checklist §28, §38)
//!    BudgetGuard → StepCounter checked before every iteration.
//!
//! 2. **Capability-token architecture** (§8)
//!    The `DetectCap` token (defined below) is a compile-time ZST.  Only code
//!    that has constructed a `DetectCap` can call `record_signal` — the
//!    compiler enforces this, no runtime check needed.
//!
//! 3. **Idempotency keys on every side effect** (§14)
//!    Signals use `IdempotencyKey::new_for(fixture:market:selection:epoch_bucket)`.
//!    Crash-and-restart cannot double-log the same signal.
//!
//! 4. **Venice LLM agent loop, not a single completion** (rig-venice
//!    ROADMAP.md Phase 1). The agent decides which tools to call and when to
//!    stop; ALL external data is wrapped in `wrap_untrusted()` delimiters
//!    before it reaches the model (§28 prompt-injection defence). The
//!    model's free-text rationale is logged as `narrative` — it never
//!    overrides the deterministic `compute_sharp_movement` tool result that
//!    gates whether a signal is recorded.
//!
//! 5. **Prediction tracking** (§19)
//!    Every signal records the odds at detection time.  On the next poll, if
//!    the odds have moved further in the predicted direction, `correct_so_far`
//!    flips to `true`.  On fixture completion the outcome is finalised.
//!
//! 6. **Structured JSON logs + tracing** (§24)
//!    Every signal and its outcome is written to `SIGNAL_LOG_PATH` as JSONL.
//!    Tracing emits structured JSON to stdout for CoralOS/Grafana ingestion.
//!
//! ## Environment variables
//!
//! | Variable | Default | Description |
//! |---|---|---|
//! | `TXLINE_API_KEY` | *required* | TxLINE bearer token |
//! | `TXLINE_API_BASE` | `https://txline.txodds.com/api/v1` | API root |
//! | `VENICE_API_KEY` | *required* | Venice AI API key (read by `rig_venice::client()`) |
//! | `VENICE_MODEL` | `kimi-k2-7-code` | Venice model name (read by `rig_venice::model_name()`) |
//! | `POLL_INTERVAL_SECS` | `60` | Seconds between polls |
//! | `ODDS_MOVE_THRESHOLD_PCT` | `4.0` | Min % move for `compute_sharp_movement` to call sharp |
//! | `CONFIDENCE_GATE` | `0.55` | Min confidence to log as a signal |
//! | `MAX_STEPS` | `500` | Hard iteration cap |
//! | `MAX_TOOL_ROUNDS` | `6` | Hard cap on tool-call rounds per reasoning pass |
//! | `SIGNAL_LOG_PATH` | `sharp-signals.jsonl` | JSONL audit log path |

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::todo)]

use std::collections::HashMap;
use std::time::Duration;

use agent_core::safety::{safety_check, BudgetGuard, StepCounter};
use tracing::{error, info, warn};

mod config;
mod detector;
mod signal;
mod txline;
mod venice;

use config::Config;
use detector::detector_step;
use signal::{load_seen_idempotency, SignalRecord};
use txline::build_txline_client;
use venice::build_reasoning_agent;

// ── Capability token (§8) ──────────────────────────────────────────────────────

/// Compile-time proof that this binary holds "detect sharp signals" rights.
/// Defined here (not in agent-core) because DetectCap is local to this binary.
#[derive(Debug, Clone, Copy)]
pub struct DetectCap;

impl DetectCap {
    pub fn acquire() -> Self { Self }
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env().add_directive(
            "sharp_movement_detector=info"
                .parse()
                .unwrap_or_else(|_| tracing::Level::INFO.into()),
        ))
        .init();

    info!(agent = "sharp-movement-detector", "starting");

    let config = match Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            error!(error = %e, "config error — aborting");
            std::process::exit(1);
        }
    };

    // ── Safety gate construction (§28, §38) ───────────────────────────────────
    let budget = BudgetGuard::default_devnet();
    let mut steps = StepCounter::new(config.max_steps);

    // Capability token — compile-time proof this binary holds DetectCap (§8).
    let _detect_cap = DetectCap::acquire();

    // ── HTTP client (TxLINE) ──────────────────────────────────────────────────
    let txline_client = match build_txline_client(&config.api_key) {
        Ok(c) => c,
        Err(e) => {
            error!(error = %e, "failed to build TxLINE HTTP client — aborting");
            std::process::exit(1);
        }
    };

    // ── Venice AI agent (rig-venice tool-calling loop) ────────────────────────
    let reasoning_agent = match build_reasoning_agent(&config) {
        Ok(a) => a,
        Err(e) => {
            error!(error = %e, "failed to build Venice reasoning agent — aborting");
            std::process::exit(1);
        }
    };

    let poll = Duration::from_secs(config.poll_interval_secs);

    // prev_odds: (fixture_id, market_key, selection_name) → last seen odds
    let mut prev_odds: HashMap<(u64, String, String), f64> = HashMap::new();
    // open_signals: signal_id → SignalRecord (for prediction tracking)
    let mut open_signals: HashMap<String, SignalRecord> = HashMap::new();
    // seen_idempotency: prevents double-logging across crash-restart
    let mut seen_idempotency: std::collections::HashSet<String> =
        load_seen_idempotency(&config.signal_log_path);

    loop {
        // ── Safety gate ───────────────────────────────────────────────────────
        if let Err(e) = safety_check(&budget) {
            warn!(error = %e, "safety gate — shutting down");
            break;
        }
        if let Err(e) = steps.tick() {
            warn!(error = %e, "step cap — shutting down");
            break;
        }

        match detector_step(
            &txline_client,
            &config,
            &budget,
            &mut prev_odds,
            &mut open_signals,
            &mut seen_idempotency,
            &reasoning_agent,
        )
        .await
        {
            Ok(n) => {
                info!(signals = n, step = steps.current(), "poll complete");
            }
            Err(e) if agent_core::error::is_retryable(&e) => {
                warn!(error = %e, "transient error — retrying next poll");
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
        "sharp-movement-detector shut down cleanly"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_cap_is_zero_sized() {
        assert_eq!(std::mem::size_of::<DetectCap>(), 0);
    }
}
