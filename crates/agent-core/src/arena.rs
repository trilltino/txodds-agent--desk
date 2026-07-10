//! Agent-vs-Agent Arena types.
//!
//! Two agents — `match-intelligence-agent` (FOLLOW strategy) and
//! `contrarian-agent` (FADE strategy) — read the same `TxLINE` feed, take
//! opposite positions on each sharp-odds-movement signal, and settle
//! on-chain after the match.  This module contains the pure domain types
//! for tracking arena state.
//!
//! The on-chain program (programs/agent-arena) uses a Borsh-serialised
//! subset of these types.  The `serde` derives here are for the Tauri/UI
//! layer and the `CoralOS` MCP schema.

use serde::{Deserialize, Serialize};

// ── Strategy enum ─────────────────────────────────────────────────────────────

/// The betting strategy an agent is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Strategy {
    /// Follow the sharp money — bet in the direction of the odds movement.
    FollowSharp,
    /// Fade the sharp money — bet against the direction of the odds movement.
    FadeSharp,
}

// ── Position ──────────────────────────────────────────────────────────────────

/// The direction of a position relative to the odds movement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PositionDirection {
    /// Agrees with the direction the odds moved.
    With,
    /// Goes against the direction the odds moved.
    Against,
}

/// A single recorded position by one agent on one signal.
///
/// Written to the audit log and the on-chain program before the match ends.
/// Checklist §24: every tool call's arguments and result are logged.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArenaPosition {
    /// Stable identifier for this position (UUID).
    pub position_id: String,
    /// The agent that recorded this position.
    pub agent_id: String,
    /// Strategy the agent is running.
    pub strategy: Strategy,
    /// `TxLINE` fixture ID.
    pub fixture_id: u64,
    /// `TxLINE` market key (e.g. "1x2", "asian\_handicap").
    pub market_key: String,
    /// The selection the agent is backing.
    pub selection: String,
    /// Decimal odds at the time the position was recorded.
    pub odds_at_entry: f64,
    /// Percentage move that triggered the signal.
    pub odds_move_pct: f64,
    /// Direction of the position relative to the odds move.
    pub direction: PositionDirection,
    /// Confidence score from the feature extractor (0.0 – 1.0).
    pub confidence: f64,
    /// ISO-8601 timestamp when the position was recorded.
    pub recorded_at: String,
    /// On-chain transaction signature (populated after the TX lands).
    pub tx_signature: Option<String>,
    /// Outcome after match completion (populated by the coordinator).
    pub outcome: Option<PositionOutcome>,
}

// ── Position outcome ──────────────────────────────────────────────────────────

/// The result of a position after the match has finished.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionOutcome {
    /// Did the selection win?
    pub selection_won: bool,
    /// Final score string, e.g. "2-1".
    pub final_score: String,
    /// Profit/loss in points (positive = profit, negative = loss).
    /// Calculated as: (odds - 1.0) if won, else -1.0.
    pub pnl_points: f64,
    /// ISO-8601 timestamp of settlement.
    pub settled_at: String,
    /// On-chain settlement TX signature.
    pub settlement_tx: Option<String>,
}

// ── Arena session ─────────────────────────────────────────────────────────────

/// The state of one arena session (typically one World Cup match).
///
/// Owned by the `arena-coordinator` agent; shared read-only with the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArenaSession {
    /// Unique session ID.
    pub session_id: String,
    /// `TxLINE` fixture ID for this match.
    pub fixture_id: u64,
    /// Human-readable fixture name, e.g. "ARG vs FRA".
    pub fixture_name: String,
    /// All positions taken by both agents in this session.
    pub positions: Vec<ArenaPosition>,
    /// Session status.
    pub status: ArenaSessionStatus,
    /// ISO-8601 start timestamp.
    pub started_at: String,
    /// ISO-8601 end timestamp (populated on settlement).
    pub ended_at: Option<String>,
}

/// Lifecycle state of an arena session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArenaSessionStatus {
    /// Agents are reading the feed and recording positions.
    Active,
    /// Match has ended; awaiting on-chain settlement.
    PendingSettlement,
    /// Settlement TX has landed; scores are final.
    Settled,
    /// Session was aborted (kill switch, budget exceeded, or coordinator error).
    Aborted,
}

// ── Leaderboard ───────────────────────────────────────────────────────────────

/// Aggregate performance of one agent across all completed arena sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentLeaderboardEntry {
    /// Unique identifier of the agent.
    pub agent_id: String,
    /// Strategy the agent runs.
    pub strategy: Strategy,
    /// Number of arena sessions completed.
    pub sessions_completed: u32,
    /// Total number of settled positions.
    pub positions_taken: u32,
    /// Number of positions where the selection won.
    pub positions_won: u32,
    /// Cumulative `PnL` in points across all settled positions.
    pub total_pnl_points: f64,
    /// Win rate as a fraction (0.0 – 1.0).
    pub win_rate: f64,
    /// Average confidence score of winning positions.
    pub avg_winning_confidence: f64,
}

impl AgentLeaderboardEntry {
    /// Derive a leaderboard entry from a slice of settled positions.
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    pub fn from_positions(
        agent_id: String,
        strategy: Strategy,
        positions: &[ArenaPosition],
    ) -> Self {
        let settled: Vec<&ArenaPosition> = positions
            .iter()
            .filter(|p| p.agent_id == agent_id && p.outcome.is_some())
            .collect();

        let sessions_completed = 0u32; // filled in by coordinator
        let positions_taken = settled.len() as u32;

        let positions_won = settled
            .iter()
            .filter(|p| p.outcome.as_ref().is_some_and(|o| o.selection_won))
            .count() as u32;

        let total_pnl_points: f64 = settled
            .iter()
            .filter_map(|p| p.outcome.as_ref().map(|o| o.pnl_points))
            .sum();

        let win_rate = if positions_taken > 0 {
            f64::from(positions_won) / f64::from(positions_taken)
        } else {
            0.0
        };

        let winning_confidences: Vec<f64> = settled
            .iter()
            .filter(|p| p.outcome.as_ref().is_some_and(|o| o.selection_won))
            .map(|p| p.confidence)
            .collect();

        let avg_winning_confidence = if winning_confidences.is_empty() {
            0.0
        } else {
            winning_confidences.iter().sum::<f64>() / winning_confidences.len() as f64
        };

        Self {
            agent_id,
            strategy,
            sessions_completed,
            positions_taken,
            positions_won,
            total_pnl_points,
            win_rate,
            avg_winning_confidence,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_position(agent_id: &str, won: bool, odds: f64, confidence: f64) -> ArenaPosition {
        let pnl = if won { odds - 1.0 } else { -1.0 };
        ArenaPosition {
            position_id: "pos-1".into(),
            agent_id: agent_id.into(),
            strategy: Strategy::FollowSharp,
            fixture_id: 1,
            market_key: "1x2".into(),
            selection: "home".into(),
            odds_at_entry: odds,
            odds_move_pct: 5.0,
            direction: PositionDirection::With,
            confidence,
            recorded_at: "2026-07-08T18:00:00Z".into(),
            tx_signature: None,
            outcome: Some(PositionOutcome {
                selection_won: won,
                final_score: "2-1".into(),
                pnl_points: pnl,
                settled_at: "2026-07-08T20:00:00Z".into(),
                settlement_tx: None,
            }),
        }
    }

    #[test]
    fn leaderboard_entry_win_rate() {
        let positions = vec![
            make_position("agent-a", true, 2.0, 0.8),
            make_position("agent-a", false, 2.0, 0.6),
            make_position("agent-a", true, 3.0, 0.9),
        ];

        let entry = AgentLeaderboardEntry::from_positions(
            "agent-a".into(),
            Strategy::FollowSharp,
            &positions,
        );

        assert_eq!(entry.positions_taken, 3);
        assert_eq!(entry.positions_won, 2);
        assert!((entry.win_rate - 2.0 / 3.0).abs() < 1e-9);
        // pnl = (2-1) + (-1) + (3-1) = 1 - 1 + 2 = 2.0
        assert!((entry.total_pnl_points - 2.0).abs() < 1e-9);
    }

    #[test]
    fn leaderboard_entry_empty_positions() {
        let entry = AgentLeaderboardEntry::from_positions(
            "agent-b".into(),
            Strategy::FadeSharp,
            &[],
        );
        assert_eq!(entry.positions_taken, 0);
        assert!((entry.win_rate - 0.0).abs() < 1e-9);
    }
}
