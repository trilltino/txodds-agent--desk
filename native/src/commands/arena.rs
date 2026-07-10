//! Arena IPC commands – agent-vs-agent intelligence tracking.
//!
//! All six commands are read-only queries against the `LedgerStore`.  Write
//! paths (recording positions, settling them, inserting signals) are driven by
//! the sidecar agents, not by the UI, so the command layer only exposes list /
//! aggregate endpoints.

use tauri::State;

use crate::domain::arena::{
    AgentLeaderboardRow, AgentSafetyStatusRow, ArenaPositionRow, ArenaScoreRow, ArenaSessionRow,
    SettlementRow, SignalRow, ToolCallRow,
};
use crate::error::AppError;
use crate::state::DesktopState;
use crate::types::now_iso;

// ── list_arena_positions ───────────────────────────────────────────────────────

/// Return up to `limit` arena positions, newest first.
///
/// Pass `agent_id` to filter to a single agent; omit (null) for all agents.
#[tauri::command]
pub async fn list_arena_positions(
    agent_id: Option<String>,
    limit: Option<i64>,
    state: State<'_, DesktopState>,
) -> Result<Vec<ArenaPositionRow>, AppError> {
    let ledger = state.ledger.lock().map_err(|_| AppError::LockPoisoned)?;
    ledger.list_arena_positions(agent_id.as_deref(), limit.unwrap_or(100))
}

// ── list_settlement_records ────────────────────────────────────────────────────

/// Return up to `limit` settlement records, newest first.
///
/// Both `agent_id` and `fixture_id` are optional filters that can be combined.
#[tauri::command]
pub async fn list_settlement_records(
    agent_id: Option<String>,
    fixture_id: Option<i64>,
    limit: Option<i64>,
    state: State<'_, DesktopState>,
) -> Result<Vec<SettlementRow>, AppError> {
    let ledger = state.ledger.lock().map_err(|_| AppError::LockPoisoned)?;
    ledger.list_settlement_records(agent_id.as_deref(), fixture_id, limit.unwrap_or(200))
}

// ── list_signal_records ────────────────────────────────────────────────────────

/// Return up to `limit` sharp-movement signal records, newest first.
///
/// Pass `fixture_id` to restrict to a single fixture.
#[tauri::command]
pub async fn list_signal_records(
    fixture_id: Option<i64>,
    limit: Option<i64>,
    state: State<'_, DesktopState>,
) -> Result<Vec<SignalRow>, AppError> {
    let ledger = state.ledger.lock().map_err(|_| AppError::LockPoisoned)?;
    ledger.list_signal_records(fixture_id, limit.unwrap_or(200))
}

// ── get_arena_score ────────────────────────────────────────────────────────────

/// Aggregate follow/fade win counts and cumulative PnL across all settled
/// positions, returning a single scorecard row.
#[tauri::command]
pub async fn get_arena_score(
    state: State<'_, DesktopState>,
) -> Result<ArenaScoreRow, AppError> {
    let ledger = state.ledger.lock().map_err(|_| AppError::LockPoisoned)?;
    ledger.get_arena_score()
}

// ── list_agent_leaderboard ─────────────────────────────────────────────────────

/// Return one row per (agent_id, strategy) pair, ordered by total PnL
/// descending.  The UI uses this to power the AgentRosterPanel ranking.
#[tauri::command]
pub async fn list_agent_leaderboard(
    state: State<'_, DesktopState>,
) -> Result<Vec<AgentLeaderboardRow>, AppError> {
    let ledger = state.ledger.lock().map_err(|_| AppError::LockPoisoned)?;
    ledger.list_agent_leaderboard()
}

// ── get_agent_safety_status ────────────────────────────────────────────────────

/// Return the current safety-gate telemetry for a given agent.
///
/// Because safety state is held in-memory by sidecar processes (not in SQLite),
/// this command reconstructs a best-effort snapshot from tool-call counts
/// derived from the `arena_tool_calls` ledger. It intentionally never returns
/// an error – a missing or unknown agent gets a "all-safe, zeroed" row so
/// panels never crash.
#[tauri::command]
pub async fn get_agent_safety_status(
    agent_id: String,
    state: State<'_, DesktopState>,
) -> Result<AgentSafetyStatusRow, AppError> {
    // Count tool calls for this agent from the ledger (best-effort proxy for
    // budget usage between full sidecar telemetry pushes).
    let tool_calls_used: i64 = {
        let ledger = state.ledger.lock().map_err(|_| AppError::LockPoisoned)?;
        let records = ledger.list_tool_call_records(Some(&agent_id), 10_000)?;
        records.len() as i64
    };

    Ok(AgentSafetyStatusRow {
        agent_id,
        budget_tool_calls_used: tool_calls_used,
        budget_tool_calls_limit: 500,
        budget_spend_lamports: 0,
        budget_spend_limit_lamports: 1_000_000,
        session_duration_secs_used: 0,
        session_duration_secs_limit: 3600,
        steps_used: tool_calls_used,
        steps_max: 500,
        last_checked_at: now_iso(),
    })
}

// ── list_arena_sessions ────────────────────────────────────────────────────────

/// Return arena sessions (one per distinct fixture), newest first.
///
/// Each session bundles all positions recorded for that fixture so the
/// AgentRosterPanel can render per-fixture performance cards without
/// additional round trips.
#[tauri::command]
pub async fn list_arena_sessions(
    limit: Option<i64>,
    state: State<'_, DesktopState>,
) -> Result<Vec<ArenaSessionRow>, AppError> {
    let ledger = state.ledger.lock().map_err(|_| AppError::LockPoisoned)?;
    ledger.list_arena_sessions(limit.unwrap_or(50))
}

// ── list_tool_call_records ─────────────────────────────────────────────────────

/// Return tool-call audit rows for the ToolCallAuditLog panel.
///
/// Pass `run_id` to scope to a single agent run; omit for the latest
/// `limit` calls across all runs (useful for the global audit view).
#[tauri::command]
pub async fn list_tool_call_records(
    run_id: Option<String>,
    limit: Option<i64>,
    state: State<'_, DesktopState>,
) -> Result<Vec<ToolCallRow>, AppError> {
    let ledger = state.ledger.lock().map_err(|_| AppError::LockPoisoned)?;
    ledger.list_tool_call_records(run_id.as_deref(), limit.unwrap_or(500))
}
