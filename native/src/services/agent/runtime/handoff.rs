//! Track-specialist delegation helpers for the multi-agent handoff step.

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

/// Produces a brief acknowledgement message from the specialist.
pub(super) fn specialist_ack(track: TrackMode, explanation: &str) -> String {
    match track {
        TrackMode::Settlement => format!(
            "settlement-agent: proof gate cleared, ready to initiate on-chain settlement. {}",
            explanation
        ),
        TrackMode::Trading => format!(
            "sharp-movement-detector: sharp signal registered, position simulation queued. {}",
            explanation
        ),
        TrackMode::Fan => format!(
            "fan-pundit-agent: fan narrative generated from intelligence package. {}",
            explanation
        ),
    }
}

/// Named `{ wager, reason }` shape for a wager ruling's Coral message/trace
/// payload — `serde_json::json!` would otherwise serialize a tuple as a bare
/// 2-element array, which is an awkward contract for the frontend to parse.
/// `None` when no ruling exists (Venice unconfigured, incomplete market,
/// etc.) — the frontend renders nothing rather than a placeholder in that
/// case.
pub(super) fn wager_ruling_payload(ruling: Option<&authority::AuthorityRuling>) -> Value {
    json!({
        "wagerRuling": ruling.map(|r| json!({ "wager": &r.wager, "reason": &r.reason }))
    })
}
