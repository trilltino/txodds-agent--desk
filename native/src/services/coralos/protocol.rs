//! Local Coral protocol helpers.
//!
//! Messages intentionally use a small, reviewable subset of the Coral market
//! grammar so the desktop app can show orchestration now and later bridge the
//! same transcript to a real CoralOS transport.

use serde_json::Value;
use uuid::Uuid;

use crate::types::{now_iso, CoralMessage, CoralSession, CoralVerb, TrackMode};

pub const USER_PROXY: &str = "user-proxy";
pub const MATCH_INTELLIGENCE_AGENT: &str = "match-intelligence-agent";
pub const PROOF_GUARD_AGENT: &str = "proof-guard-agent";
pub const SETTLEMENT_AGENT: &str = "settlement-agent";
pub const SHARP_MOVEMENT_DETECTOR_AGENT: &str = "sharp-movement-detector";
pub const FAN_PUNDIT_AGENT: &str = "fan-pundit-agent";
pub const TXLINE_INGEST: &str = "txline-ingest";
pub const TXLINE_NORMALIZER: &str = "txline-normalizer";
pub const FEATURE_EXTRACTOR: &str = "feature-extractor";

/// All named participants registered in every CoralOS session.
pub const ALL_AGENTS: &[&str] = &[
    MATCH_INTELLIGENCE_AGENT,
    PROOF_GUARD_AGENT,
    SETTLEMENT_AGENT,
    SHARP_MOVEMENT_DETECTOR_AGENT,
    FAN_PUNDIT_AGENT,
    TXLINE_INGEST,
    TXLINE_NORMALIZER,
    FEATURE_EXTRACTOR,
    USER_PROXY,
];

pub fn start_session(run_id: &str, fixture_id: u64, track: TrackMode) -> CoralSession {
    let id = format!("coral-{run_id}");
    CoralSession {
        thread_id: format!("{id}:main"),
        id,
        fixture_id,
        track,
        created_at: now_iso(),
    }
}

pub fn message(
    session: &CoralSession,
    round: u64,
    from: impl Into<String>,
    to: Vec<&str>,
    verb: CoralVerb,
    text: impl Into<String>,
    payload: Option<Value>,
) -> CoralMessage {
    CoralMessage {
        id: format!("msg-{}", Uuid::new_v4()),
        session_id: session.id.clone(),
        thread_id: session.thread_id.clone(),
        round,
        from: from.into(),
        to: to.into_iter().map(ToString::to_string).collect(),
        verb,
        text: text.into(),
        payload,
        ts: now_iso(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_ids_are_run_scoped() {
        let session = start_session("run-1", 7, TrackMode::Trading);
        assert_eq!(session.id, "coral-run-1");
        assert_eq!(session.thread_id, "coral-run-1:main");
        assert_eq!(session.fixture_id, 7);
    }
}
