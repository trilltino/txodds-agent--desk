//! Native arena domain types for the Tauri IPC layer.
//!
//! These are flat, serde-serialisable projections of the rich types in
//! `crates/agent-core/src/arena.rs`.  The IPC layer works with these rows
//! rather than the full `ArenaPosition` / `AgentLeaderboardEntry` structs so
//! that the UI receives stable `camelCase` JSON objects and the SQLite schema
//! stays simple (everything stored as `TEXT`/`REAL`/`INTEGER`).

use serde::{Deserialize, Serialize};

// ── Arena position (one bet taken by one agent) ───────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArenaPositionRow {
    pub position_id: String,
    pub agent_id: String,
    /// "follow_sharp" | "fade_sharp"
    pub strategy: String,
    pub fixture_id: i64,
    pub market_key: String,
    pub selection: String,
    pub odds_at_entry: f64,
    pub odds_move_pct: f64,
    /// "with" | "against"
    pub direction: String,
    /// 0.0 – 1.0
    pub confidence: f64,
    pub recorded_at: String,
    pub tx_signature: Option<String>,
    // Populated after settlement.
    pub outcome_won: Option<bool>,
    pub outcome_pnl: Option<f64>,
    pub outcome_settled_at: Option<String>,
}

// ── Settlement record (one settled position) ──────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettlementRow {
    pub idempotency_key: String,
    pub fixture_id: i64,
    pub agent_id: String,
    /// "FollowSharp" | "FadeSharp"
    pub strategy: String,
    pub market_key: String,
    pub selection: String,
    /// "With" | "Against"
    pub direction: String,
    pub odds_at_entry: f64,
    /// "win" | "loss"
    pub result: String,
    pub pnl_units: f64,
    pub settled_at: String,
}

// ── Signal record (one detected sharp-movement signal) ────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalRow {
    pub signal_id: String,
    pub idempotency_key: String,
    pub fixture_id: i64,
    pub fixture_name: String,
    pub market_key: String,
    pub selection: String,
    pub odds_now: f64,
    pub odds_prev: f64,
    pub move_pct: f64,
    /// "shortened" | "lengthened"
    pub direction: String,
    /// 0.0 – 1.0
    pub confidence: f64,
    pub detected_at: String,
    pub narrative: Option<String>,
    pub correct_so_far: bool,
}

// ── Arena score (aggregate across all settled positions) ──────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArenaScoreRow {
    pub follow_wins: i64,
    pub follow_losses: i64,
    pub fade_wins: i64,
    pub fade_losses: i64,
    pub follow_pnl: f64,
    pub fade_pnl: f64,
    /// "FOLLOW (match-intelligence)" | "FADE (contrarian)" | "TIE"
    pub leader: String,
}

// ── Agent leaderboard (one row per agent) ─────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentLeaderboardRow {
    pub agent_id: String,
    pub strategy: String,
    pub positions_taken: i64,
    pub positions_won: i64,
    pub total_pnl_points: f64,
    pub win_rate: f64,
    pub avg_winning_confidence: f64,
}

// ── Agent safety status (in-memory runtime telemetry) ────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSafetyStatusRow {
    pub agent_id: String,
    pub budget_tool_calls_used: i64,
    pub budget_tool_calls_limit: i64,
    pub budget_spend_lamports: i64,
    pub budget_spend_limit_lamports: i64,
    pub session_duration_secs_used: i64,
    pub session_duration_secs_limit: i64,
    pub steps_used: i64,
    pub steps_max: i64,
    pub last_checked_at: String,
}

// ── Arena session (one World Cup fixture, with nested positions) ───────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArenaSessionRow {
    pub session_id: String,
    pub fixture_id: i64,
    pub fixture_name: String,
    /// All positions recorded for this fixture, oldest first.
    pub positions: Vec<ArenaPositionRow>,
    /// "active" | "pending_settlement" | "settled" | "aborted"
    pub status: String,
    pub started_at: String,
    pub ended_at: Option<String>,
}

// ── Backtest settlement (one simulated position, replayed against history) ───
//
// Deliberately a separate row/table from SettlementRow/arena_settlements —
// see the schema comment in ledger/store.rs for why backtest results must
// never be queryable through the same commands as live-tournament results.

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BacktestSettlementRow {
    pub position_id: String,
    pub fixture_id: i64,
    pub fixture_home: String,
    pub fixture_away: String,
    pub agent_id: String,
    /// "FollowSharp" | "FadeSharp"
    pub strategy: String,
    pub market_key: String,
    pub selection: String,
    /// "With" | "Against"
    pub direction: String,
    pub odds_at_entry: f64,
    pub odds_move_pct: f64,
    pub confidence: f64,
    /// "win" | "loss"
    pub result: String,
    pub pnl_units: f64,
    pub final_score: String,
    pub recorded_at: String,
    pub settled_at: String,
}

// ── Tool-call audit row ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallRow {
    pub id: String,
    pub run_id: String,
    pub tool_name: String,
    pub arguments_json: String,
    pub result_json: Option<String>,
    /// "ok" | "error" | "pending"
    pub status: String,
    pub created_at: String,
}
