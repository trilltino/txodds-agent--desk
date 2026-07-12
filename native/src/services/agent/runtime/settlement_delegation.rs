//! Real delegation of on-chain settlement to `settlement-agent` — a
//! genuinely independent OS process, not an in-process function call.
//!
//! Mirrors `wager_proof_delegation`'s shape exactly: POST a
//! `SETTLE_REQUESTED` message to the live thread, poll for
//! `settlement-agent`'s own `SETTLE_VERDICT` reply, fail closed on timeout.
//! There is no local fallback settlement computation — the whole point is
//! that `native/` no longer decides this itself, and only `settlement-agent`
//! holds the `SettleCap` capability token that would let it act.

use std::time::Duration;

use agent_core::ToolTrailEntry;
use coral_client::wire;
use txodds_types::wager::Wager;

use crate::config::AppConfig;
use crate::services::coralos::console::{self, LiveSession};
use crate::services::coralos::protocol::SETTLEMENT_AGENT;
use crate::services::coralos::thread_reader;
use crate::types::AgentRun;

use super::handoff::tool_trail_wire_suffix;

const WAIT_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(750);

pub struct RealSettlementOutcome {
    pub status: String,
    pub reason: String,
    pub tx_sig: Option<String>,
}

/// Send `wager` to the real `settlement-agent` process and wait for its
/// verdict. Returns `status="unreachable"` (never `"settled"`) if the
/// wager has no `proof_ref` yet, if the CoralOS console is disabled, or if
/// the process doesn't reply in time — fail closed, same standard as
/// `wager_proof_delegation`.
pub async fn delegate(
    client: &reqwest::Client,
    config: &AppConfig,
    run: &AgentRun,
    live: &mut Option<LiveSession>,
    wager: &Wager,
    tool_trail: &[ToolTrailEntry],
) -> RealSettlementOutcome {
    let Some(proof_ref) = wager.proof_ref.clone() else {
        return unreachable_outcome("wager has no proof_ref yet — cannot request settlement");
    };

    if !config.coralos_console_enabled {
        return unreachable_outcome(
            "CoralOS console disabled (CORALOS_CONSOLE_ENABLED=0) — cannot reach settlement-agent",
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
                    "CoralOS session unavailable, cannot reach settlement-agent: {err}"
                ))
            }
        },
    };

    let wager_json = match serde_json::to_string(wager) {
        Ok(json) => json,
        Err(err) => return unreachable_outcome(&format!("failed to serialize wager: {err}")),
    };
    // `toolTrail=` before the trailing `wager=` — see
    // `tool_trail_wire_suffix`'s ordering contract.
    let content = format!(
        "SETTLE_REQUESTED proofRef={proof_ref}{} wager={wager_json}",
        tool_trail_wire_suffix(tool_trail),
    );

    if let Err(err) =
        console::send_raw_message(client, config, &session, &content, &[SETTLEMENT_AGENT]).await
    {
        return unreachable_outcome(&format!("failed to delegate to settlement-agent: {err}"));
    }

    let contains = format!("wagerId={}", wager.wager_id);
    let reply = thread_reader::wait_for_message(
        client,
        config,
        &session,
        SETTLEMENT_AGENT,
        "SETTLE_VERDICT",
        &contains,
        WAIT_TIMEOUT,
        POLL_INTERVAL,
    )
    .await;

    let Some(reply) = reply else {
        return unreachable_outcome(&format!(
            "settlement-agent did not respond within {}s — treating as unreachable (fail closed)",
            WAIT_TIMEOUT.as_secs()
        ));
    };

    let status = wire::tok(&reply.text, "status").unwrap_or("unreachable").to_string();
    let reason = wire::quoted(&reply.text, "reason")
        .unwrap_or_else(|| "settlement-agent replied without a reason".to_string());
    let tx_sig = wire::tok(&reply.text, "txSig")
        .filter(|s| *s != "none")
        .map(ToString::to_string);

    RealSettlementOutcome {
        status,
        reason,
        tx_sig,
    }
}

fn unreachable_outcome(reason: &str) -> RealSettlementOutcome {
    RealSettlementOutcome {
        status: "unreachable".to_string(),
        reason: reason.to_string(),
        tx_sig: None,
    }
}
