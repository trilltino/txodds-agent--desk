//! Signal record types, the JSONL audit log, and prediction tracking.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tracing::warn;

/// Direction the odds moved (from this agent's perspective).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OddsDirection {
    /// Odds shortened — sharp money flowing TO this selection.
    Shortened,
    /// Odds lengthened — sharp money flowing AWAY from this selection.
    Lengthened,
}

impl OddsDirection {
    /// Parse the `direction` string produced by the `compute_sharp_movement`
    /// tool (`"shortening"` / `"drifting"`), which uses different wording
    /// than this enum's own serde representation.
    pub(crate) fn from_tool_str(s: &str) -> Option<Self> {
        match s {
            "shortening" => Some(Self::Shortened),
            "drifting" => Some(Self::Lengthened),
            _ => None,
        }
    }
}

/// One detected sharp-movement signal, logged to the JSONL audit trail.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SignalRecord {
    /// Idempotency key — prevents double-logging on restart (§14).
    pub(crate) idempotency_key: String,
    /// Unique signal ID.
    pub(crate) signal_id: String,
    /// TxLINE fixture ID.
    pub(crate) fixture_id: u64,
    /// Human-readable fixture name.
    pub(crate) fixture_name: String,
    /// TxLINE market key.
    pub(crate) market_key: String,
    /// Selection name.
    pub(crate) selection: String,
    /// Odds at time of detection.
    pub(crate) odds_now: f64,
    /// Odds in the previous poll.
    pub(crate) odds_prev: f64,
    /// Absolute percentage move, as computed by the `compute_sharp_movement` tool.
    pub(crate) move_pct: f64,
    /// Direction of the move.
    pub(crate) direction: OddsDirection,
    /// Confidence score (0.0 – 1.0), as computed by the `compute_sharp_movement` tool.
    pub(crate) confidence: f64,
    /// ISO-8601 Unix-epoch timestamp of detection.
    pub(crate) detected_at: String,
    /// Venice agent's free-text rationale for this signal.  Advisory only —
    /// NEVER used to decide whether a position is taken.
    pub(crate) narrative: Option<String>,
    /// True if odds continued moving in the same direction on the next poll.
    pub(crate) correct_so_far: bool,
    /// Final outcome once the fixture is complete.
    pub(crate) outcome: Option<String>,
}

/// For each open signal, check if the odds have continued in the predicted
/// direction.  Update `correct_so_far` in the in-memory map.
pub(crate) fn update_open_signals(
    open_signals: &mut HashMap<String, SignalRecord>,
    current_odds: &HashMap<(u64, String, String), f64>,
    log_path: &str,
) {
    for signal in open_signals.values_mut() {
        let key = (
            signal.fixture_id,
            signal.market_key.clone(),
            signal.selection.clone(),
        );
        if let Some(&current) = current_odds.get(&key) {
            let still_moving_same_way = match signal.direction {
                OddsDirection::Shortened => current <= signal.odds_now,
                OddsDirection::Lengthened => current >= signal.odds_now,
            };
            if still_moving_same_way != signal.correct_so_far {
                signal.correct_so_far = still_moving_same_way;
                // Re-append with updated state (JSONL is append-only; the last
                // record with this signal_id wins on replay).
                if let Err(e) = append_signal(log_path, signal) {
                    warn!(error = %e, signal_id = %signal.signal_id, "failed to update signal");
                }
            }
        }
    }
}

pub(crate) fn append_signal(path: &str, record: &SignalRecord) -> Result<(), agent_core::error::AgentError> {
    use agent_core::error::AgentError;
    use std::io::Write;
    let line =
        serde_json::to_string(record).map_err(|e| AgentError::ParseError(e.to_string()))?;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| AgentError::ToolCallFailed {
            tool: "append_signal".into(),
            reason: e.to_string(),
        })?;
    writeln!(f, "{line}").map_err(|e| AgentError::ToolCallFailed {
        tool: "append_signal".into(),
        reason: e.to_string(),
    })
}

/// Load idempotency keys from an existing log so a restarted agent does not
/// re-log signals it has already recorded in a previous run.  Checklist §14.
pub(crate) fn load_seen_idempotency(path: &str) -> std::collections::HashSet<String> {
    #[derive(Deserialize)]
    struct Partial {
        idempotency_key: String,
    }
    let Ok(content) = std::fs::read_to_string(path) else {
        return std::collections::HashSet::new();
    };
    content
        .lines()
        .filter_map(|l| serde_json::from_str::<Partial>(l).ok())
        .map(|p| p.idempotency_key)
        .collect()
}

pub(crate) fn unix_now() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub(crate) fn unix_now_iso() -> String {
    format!("{}Z", unix_now())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn load_seen_idempotency_handles_missing_file() {
        let seen = load_seen_idempotency("/tmp/non_existent_sharp_signals_test.jsonl");
        assert!(seen.is_empty());
    }

    #[test]
    fn direction_from_tool_str_parses_known_values() {
        assert_eq!(OddsDirection::from_tool_str("shortening"), Some(OddsDirection::Shortened));
        assert_eq!(OddsDirection::from_tool_str("drifting"), Some(OddsDirection::Lengthened));
        assert_eq!(OddsDirection::from_tool_str("sideways"), None);
    }

    #[test]
    fn update_open_signals_flips_correct_so_far() {
        let signal_id = Uuid::new_v4().to_string();
        let mut open: HashMap<String, SignalRecord> = HashMap::new();
        open.insert(
            signal_id.clone(),
            SignalRecord {
                idempotency_key: "idem-test".into(),
                signal_id: signal_id.clone(),
                fixture_id: 42,
                fixture_name: "Test FC vs Mock United".into(),
                market_key: "1x2".into(),
                selection: "home".into(),
                odds_now: 2.00,
                odds_prev: 2.10,
                move_pct: 4.76,
                direction: OddsDirection::Shortened,
                confidence: 0.60,
                detected_at: "1720476000Z".into(),
                narrative: None,
                correct_so_far: false,
                outcome: None,
            },
        );

        // Current odds have shortened further (1.90 < 2.00) → correct_so_far = true.
        let mut current: HashMap<(u64, String, String), f64> = HashMap::new();
        current.insert((42, "1x2".into(), "home".into()), 1.90);

        // Write to a temp path (won't exist) — update will log a warn but not panic.
        update_open_signals(&mut open, &current, "/tmp/test_sharp_update.jsonl");

        assert!(open[&signal_id].correct_so_far);
    }
}
