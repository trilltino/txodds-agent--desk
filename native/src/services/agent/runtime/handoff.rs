//! Track-specialist delegation helpers for the multi-agent handoff step.

use agent_core::ToolTrailEntry;
use serde_json::{json, Value};

use crate::services::coralos::protocol::{
    FAN_PUNDIT_AGENT, SETTLEMENT_AGENT, SHARP_MOVEMENT_DETECTOR_AGENT,
};
use crate::types::TrackMode;

use super::super::authority;

/// Returns the specialist agent name for the given track.
pub(super) fn specialist_for_track(track: TrackMode) -> &'static str {
    match track {
        TrackMode::Settlement => SETTLEMENT_AGENT,
        TrackMode::Trading => SHARP_MOVEMENT_DETECTOR_AGENT,
        TrackMode::Fan => FAN_PUNDIT_AGENT,
    }
}

/// Named `{ wager, reason }` shape for a wager ruling's Coral message/trace
/// payload — `serde_json::json!` would otherwise serialize a tuple as a bare
/// 2-element array, which is an awkward contract for the frontend to parse.
/// `None` when no ruling exists (Venice unconfigured, incomplete market,
/// etc.) — the frontend renders nothing rather than a placeholder in that
/// case.
///
/// `tool_trail` is the reasoning pass's real tool-call trace (TODO 6e) —
/// which tools the Venice loop called and what they returned — so the
/// Console transcript shows *why* the agent concluded what it did, not just
/// the verdict. Empty slice → `"toolTrail": []`, rendered as nothing.
pub(super) fn wager_ruling_payload(
    ruling: Option<&authority::AuthorityRuling>,
    tool_trail: &[ToolTrailEntry],
) -> Value {
    json!({
        "wagerRuling": ruling.map(|r| json!({ "wager": &r.wager, "reason": &r.reason })),
        "toolTrail": tool_trail
    })
}

/// Serialize a reasoning trail as the ` toolTrail=<json>` wire suffix for a
/// delegation message, or an empty string when there is nothing to carry.
///
/// Ordering contract: this suffix goes **before** the trailing
/// `wager=<json>` token. `wager=` stays the last key on every message so
/// specialists still running the old greedy-to-end-of-string extractor keep
/// parsing correctly; the current extractor (`coral_client::wire::json_val`)
/// does not care about order.
pub(super) fn tool_trail_wire_suffix(tool_trail: &[ToolTrailEntry]) -> String {
    if tool_trail.is_empty() {
        return String::new();
    }
    match serde_json::to_string(tool_trail) {
        Ok(json) => format!(" toolTrail={json}"),
        Err(_) => String::new(),
    }
}
