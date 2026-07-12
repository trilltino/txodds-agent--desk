//! Real delegation of the wager-consistency proof check to `proof-guard-agent`
//! — a genuinely independent OS process, not an in-process function call.
//!
//! This is the one piece of `native/` that becomes dependent on another
//! process's real output: it POSTs a `WAGER_PROOF_REQUESTED` message to a
//! live CoralOS thread and polls for `proof-guard-agent`'s own
//! `WAGER_PROOF_VERDICT` reply (`coralos::thread_reader`). If that process
//! never replies — killed, never registered, wrong image — this times out
//! and fails closed (`WagerStatus::ProofFailed`); there is no local
//! fallback computation, because the whole point of this module is that
//! `native/` no longer computes this verdict itself.

use std::time::Duration;

use agent_core::ToolTrailEntry;
use coral_client::wire;
use txodds_types::wager::{Wager, WagerStatus};

use crate::config::AppConfig;
use crate::services::coralos::console::{self, LiveSession};
use crate::services::coralos::protocol::{MATCH_INTELLIGENCE_AGENT, PROOF_GUARD_AGENT};
use crate::services::coralos::thread_reader;
use crate::types::AgentRun;

use super::handoff::tool_trail_wire_suffix;

const WAIT_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(750);

pub struct RealProofGuardOutcome {
    pub wager: Wager,
    pub reason: String,
}

/// Send `wager` to the real `proof-guard-agent` process over a live CoralOS
/// thread (creating the thread on first use, reused for the rest of the
/// round) and wait for its verdict. `client`/`config` come from
/// `DesktopState`; `live` is threaded through the whole round so
/// `console::publish_run`'s end-of-round batch reuses the same session
/// instead of creating a second one.
pub async fn delegate(
    client: &reqwest::Client,
    config: &AppConfig,
    run: &AgentRun,
    live: &mut Option<LiveSession>,
    wager: &Wager,
    tool_trail: &[ToolTrailEntry],
) -> RealProofGuardOutcome {
    if !config.coralos_console_enabled {
        return fail_closed(wager, "CoralOS console disabled (CORALOS_CONSOLE_ENABLED=0) — cannot reach proof-guard-agent");
    }

    let session = match live {
        Some(session) => session.clone(),
        None => match console::ensure_session(client, config, run).await {
            Ok(session) => {
                *live = Some(session.clone());
                session
            }
            Err(err) => {
                return fail_closed(
                    wager,
                    &format!("CoralOS session unavailable, cannot reach proof-guard-agent: {err}"),
                )
            }
        },
    };

    let wager_json = match serde_json::to_string(wager) {
        Ok(json) => json,
        Err(err) => return fail_closed(wager, &format!("failed to serialize wager: {err}")),
    };
    // `toolTrail=` (the reasoning that produced this wager, TODO 6e) goes
    // before `wager=`, which must stay the trailing key — see
    // `tool_trail_wire_suffix`'s ordering contract.
    let content = format!(
        "WAGER_PROOF_REQUESTED round={} wagerId={}{} wager={wager_json}",
        wager.fixture_id,
        wager.wager_id,
        tool_trail_wire_suffix(tool_trail),
    );

    if let Err(err) = console::send_raw_message(
        client,
        config,
        &session,
        &content,
        &[PROOF_GUARD_AGENT],
    )
    .await
    {
        return fail_closed(
            wager,
            &format!("failed to delegate to proof-guard-agent: {err}"),
        );
    }

    let contains = format!("wagerId={}", wager.wager_id);
    let reply = thread_reader::wait_for_message(
        client,
        config,
        &session,
        PROOF_GUARD_AGENT,
        "WAGER_PROOF_VERDICT",
        &contains,
        WAIT_TIMEOUT,
        POLL_INTERVAL,
    )
    .await;

    let Some(reply) = reply else {
        return fail_closed(
            wager,
            &format!(
                "proof-guard-agent did not respond within {}s — treating as failed (fail closed)",
                WAIT_TIMEOUT.as_secs()
            ),
        );
    };

    match parse_verdict_wager(&reply.text) {
        Some(updated_wager) => RealProofGuardOutcome {
            reason: format!(
                "{}: real proof-guard-agent verdict — {:?}",
                MATCH_INTELLIGENCE_AGENT, updated_wager.status
            ),
            wager: updated_wager,
        },
        None => fail_closed(
            wager,
            &format!("proof-guard-agent reply was malformed: {}", reply.text),
        ),
    }
}

fn parse_verdict_wager(text: &str) -> Option<Wager> {
    serde_json::from_str(wire::json_val(text, "wager")?).ok()
}

fn fail_closed(wager: &Wager, reason: &str) -> RealProofGuardOutcome {
    let mut wager = wager.clone();
    wager.status = WagerStatus::ProofFailed;
    RealProofGuardOutcome {
        wager,
        reason: reason.to_string(),
    }
}
