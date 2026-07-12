//! Backtest IPC commands — replay a completed fixture's real TxLINE odds
//! history and settle simulated FollowSharp/FadeSharp positions against its
//! real final score. See `services::backtest` for the engine and
//! ARENA-AUTONOMY-PLAN.md for the design rationale.

use tauri::State;

use crate::domain::arena::BacktestSettlementRow;
use crate::error::AppError;
use crate::services::backtest::{self, BacktestSummary};
use crate::state::DesktopState;

/// Replay `fixture_id`'s real historical odds (fetched hour-by-hour, never
/// the whole-fixture endpoint — see `services::backtest` module docs) and
/// settle simulated positions against its real final score.
///
/// `home`/`away`/`kickoff_ts_ms` come from the caller — the UI already has
/// these from the fixture board, so this avoids a redundant fixture lookup.
/// Fails with `AppError::InvalidInput` if the fixture has no final score yet
/// (a backtest needs a completed match).
#[tauri::command]
pub async fn run_backtest(
    fixture_id: u64,
    home: String,
    away: String,
    kickoff_ts_ms: i64,
    state: State<'_, DesktopState>,
) -> Result<BacktestSummary, AppError> {
    let summary = backtest::replay_fixture(&state.client, &state.config, fixture_id, &home, &away, kickoff_ts_ms).await?;

    let mut ledger = state.ledger.lock().map_err(|_| AppError::LockPoisoned)?;
    ledger.replace_backtest_settlements(
        fixture_id.try_into().map_err(|_| AppError::InvalidInput("fixture_id too large".to_string()))?,
        &home,
        &away,
        &summary.positions,
    )?;

    Ok(summary)
}

/// List persisted backtest settlement rows, optionally scoped to one fixture.
#[tauri::command]
pub async fn list_backtest_settlements(
    fixture_id: Option<i64>,
    state: State<'_, DesktopState>,
) -> Result<Vec<BacktestSettlementRow>, AppError> {
    let ledger = state.ledger.lock().map_err(|_| AppError::LockPoisoned)?;
    ledger.list_backtest_settlements(fixture_id)
}
