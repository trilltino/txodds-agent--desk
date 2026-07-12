//! Real delegation of the narrative-reaction handoff to `fan-pundit-agent`
//! — a genuinely independent OS process, not an in-process function call.
//!
//! This is distinct from `pundit_agent::react_to_wager` (still an in-process
//! Venice call, part of the wager debate itself — Phase 5 of
//! `crates/rig-venice/ROADMAP.md`, run unconditionally regardless of track).
//! This module is specifically the Fan-track specialist handoff that used
//! to be `handoff::specialist_ack`'s scripted stub text — now a real
//! delegation to the same real logic `fan-pundit-agent`'s binary runs,
//! mirroring `wager_proof_delegation`'s shape.

use std::time::Duration;

use agent_core::ToolTrailEntry;
use coral_client::wire;
use txodds_types::wager::Wager;

use crate::config::AppConfig;
use crate::services::coralos::console::{self, LiveSession};
use crate::services::coralos::protocol::FAN_PUNDIT_AGENT;
use crate::services::coralos::thread_reader;
use crate::types::AgentRun;

use super::handoff::tool_trail_wire_suffix;

const WAIT_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(750);

pub struct RealPunditOutcome {
    pub wager: Wager,
    pub stance: String,
    pub reason: String,
}

/// Send `wager` to the real `fan-pundit-agent` process and wait for its
/// verdict. Fails closed to `stance="unreachable"` (leaving `wager`
/// unchanged) if the process doesn't reply in time.
pub async fn delegate(
    client: &reqwest::Client,
    config: &AppConfig,
    run: &AgentRun,
    live: &mut Option<LiveSession>,
    wager: &Wager,
    tool_trail: &[ToolTrailEntry],
) -> RealPunditOutcome {
    if !config.coralos_console_enabled {
        return unreachable_outcome(
            wager,
            "CoralOS console disabled (CORALOS_CONSOLE_ENABLED=0) — cannot reach fan-pundit-agent",
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
                return unreachable_outcome(
                    wager,
                    &format!("CoralOS session unavailable, cannot reach fan-pundit-agent: {err}"),
                )
            }
        },
    };

    let wager_json = match serde_json::to_string(wager) {
        Ok(json) => json,
        Err(err) => return unreachable_outcome(wager, &format!("failed to serialize wager: {err}")),
    };
    // `toolTrail=` before the trailing `wager=` — see
    // `tool_trail_wire_suffix`'s ordering contract.
    let content = format!(
        "PUNDIT_REACT_REQUESTED{} wager={wager_json}",
        tool_trail_wire_suffix(tool_trail),
    );

    if let Err(err) =
        console::send_raw_message(client, config, &session, &content, &[FAN_PUNDIT_AGENT]).await
    {
        return unreachable_outcome(
            wager,
            &format!("failed to delegate to fan-pundit-agent: {err}"),
        );
    }

    let contains = format!("wagerId\":\"{}", wager.wager_id);
    let reply = thread_reader::wait_for_message(
        client,
        config,
        &session,
        FAN_PUNDIT_AGENT,
        "PUNDIT_REACT_VERDICT",
        &contains,
        WAIT_TIMEOUT,
        POLL_INTERVAL,
    )
    .await;

    let Some(reply) = reply else {
        return unreachable_outcome(
            wager,
            &format!(
                "fan-pundit-agent did not respond within {}s — treating as unreachable (fail closed)",
                WAIT_TIMEOUT.as_secs()
            ),
        );
    };

    let stance = wire::tok(&reply.text, "stance").unwrap_or("unreachable").to_string();
    let reason = wire::quoted(&reply.text, "reason")
        .unwrap_or_else(|| "fan-pundit-agent replied without a reason".to_string());
    let updated_wager = parse_verdict_wager(&reply.text).unwrap_or_else(|| wager.clone());

    RealPunditOutcome {
        wager: updated_wager,
        stance,
        reason,
    }
}

fn parse_verdict_wager(text: &str) -> Option<Wager> {
    serde_json::from_str(wire::json_val(text, "wager")?).ok()
}

fn unreachable_outcome(wager: &Wager, reason: &str) -> RealPunditOutcome {
    RealPunditOutcome {
        wager: wager.clone(),
        stance: "unreachable".to_string(),
        reason: reason.to_string(),
    }
}
