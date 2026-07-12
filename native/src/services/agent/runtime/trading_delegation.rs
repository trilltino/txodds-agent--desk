//! Real delegation of the Trading-track handoff to the `sharp-movement-detector`
//! CoralOS participant — a genuinely independent OS process (built as the
//! `trading-specialist` crate, see that crate's docs for the naming note),
//! not an in-process function call. Mirrors `wager_proof_delegation`'s
//! shape.

use std::time::Duration;

use agent_core::domain::{AgentDecision, AgentSignal};
use coral_client::wire;

use crate::config::AppConfig;
use crate::services::coralos::console::{self, LiveSession};
use crate::services::coralos::protocol::SHARP_MOVEMENT_DETECTOR_AGENT;
use crate::services::coralos::thread_reader;
use crate::types::AgentRun;

const WAIT_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(750);

pub struct RealTradeOutcome {
    pub status: String,
    pub reason: String,
    pub position_id: Option<String>,
    pub size_sol: f64,
}

/// Send `signal`/`decision` to the real Trading specialist process and wait
/// for its verdict. Fails closed to `status="unreachable"` if the process
/// doesn't reply in time — no local fallback position simulation.
pub async fn delegate(
    client: &reqwest::Client,
    config: &AppConfig,
    run: &AgentRun,
    live: &mut Option<LiveSession>,
    signal: &AgentSignal,
    decision: &AgentDecision,
) -> RealTradeOutcome {
    if !config.coralos_console_enabled {
        return unreachable_outcome(
            "CoralOS console disabled (CORALOS_CONSOLE_ENABLED=0) — cannot reach sharp-movement-detector",
        );
    }

    let session = match live {
        Some(session) => session.clone(),
        None => match console::ensure_session(client, config, run).await {
            Ok(session) => {
                *live = Some(session.clone());
                session
            }
            Err(err) => {
                return unreachable_outcome(&format!(
                    "CoralOS session unavailable, cannot reach sharp-movement-detector: {err}"
                ))
            }
        },
    };

    let (signal_json, decision_json) =
        match (serde_json::to_string(signal), serde_json::to_string(decision)) {
            (Ok(s), Ok(d)) => (s, d),
            _ => return unreachable_outcome("failed to serialize signal/decision"),
        };
    let content = format!("TRADE_REQUESTED signal={signal_json} decision={decision_json}");

    if let Err(err) = console::send_raw_message(
        client,
        config,
        &session,
        &content,
        &[SHARP_MOVEMENT_DETECTOR_AGENT],
    )
    .await
    {
        return unreachable_outcome(&format!(
            "failed to delegate to sharp-movement-detector: {err}"
        ));
    }

    // TRADE_VERDICT carries no correlation token of its own (see
    // trading-specialist's wire grammar — it never echoes the delegated
    // signal's id). Only one delegation is ever in flight per round, so
    // matching on the verb alone (empty `contains`) is sufficient; unlike
    // `wager_proof_delegation`/`fan_pundit_delegation`, there's no id to
    // scope the wait to.
    let reply = thread_reader::wait_for_message(
        client,
        config,
        &session,
        SHARP_MOVEMENT_DETECTOR_AGENT,
        "TRADE_VERDICT",
        "",
        WAIT_TIMEOUT,
        POLL_INTERVAL,
    )
    .await;

    let Some(reply) = reply else {
        return unreachable_outcome(&format!(
            "sharp-movement-detector did not respond within {}s — treating as unreachable (fail closed)",
            WAIT_TIMEOUT.as_secs()
        ));
    };

    let status = wire::tok(&reply.text, "status").unwrap_or("unreachable").to_string();
    let reason = wire::quoted(&reply.text, "reason")
        .unwrap_or_else(|| "sharp-movement-detector replied without a reason".to_string());
    let position_id = wire::tok(&reply.text, "positionId")
        .filter(|s| *s != "none")
        .map(ToString::to_string);
    let size_sol = wire::num(&reply.text, "sizeSol").unwrap_or(0.0);

    RealTradeOutcome {
        status,
        reason,
        position_id,
        size_sol,
    }
}

fn unreachable_outcome(reason: &str) -> RealTradeOutcome {
    RealTradeOutcome {
        status: "unreachable".to_string(),
        reason: reason.to_string(),
        position_id: None,
        size_sol: 0.0,
    }
}
