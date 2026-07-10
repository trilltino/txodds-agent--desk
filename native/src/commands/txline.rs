//! TxLINE commands: ingest lifecycle plus the documented data endpoints.
//!
//! Every data command routes through `services::txline::api::authenticated_get`,
//! which enforces the documented-endpoint allowlist and keeps the guest JWT and
//! API token on the Rust side.

use serde_json::Value;
use tauri::{AppHandle, State};

use crate::error::AppError;
use crate::services::txline;
use crate::state::DesktopState;

#[tauri::command]
pub async fn start_txline(
    mode: String,
    fixture_id: Option<String>,
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<(), AppError> {
    if mode != "live" {
        return Err(AppError::InvalidInput(
            "TxLINE ingest is live-only in the desktop app".to_string(),
        ));
    }

    // Only one TxLINE task should own event emission at a time.
    {
        let mut task = state
            .txline_task
            .lock()
            .map_err(|_| AppError::Task("txline task lock poisoned".to_string()))?;
        if let Some(handle) = task.take() {
            handle.abort();
        }
    }
    let handle = txline::spawn_txline(
        app,
        state.client.clone(),
        state.config.clone(),
        mode,
        fixture_id,
        state.replay_dir.clone(),
    );
    let mut task = state
        .txline_task
        .lock()
        .map_err(|_| AppError::Task("txline task lock poisoned".to_string()))?;
    *task = Some(handle);
    Ok(())
}

#[tauri::command]
pub fn stop_txline(state: State<'_, DesktopState>) -> Result<(), AppError> {
    // Abort is acceptable for ingest streams because events are append-only and
    // replay writes happen before emission.
    let mut task = state
        .txline_task
        .lock()
        .map_err(|_| AppError::Task("txline task lock poisoned".to_string()))?;
    if let Some(handle) = task.take() {
        handle.abort();
    }
    Ok(())
}

#[tauri::command]
pub async fn txline_fixtures_snapshot(
    start_epoch_day: Option<u64>,
    competition_id: Option<u64>,
    state: State<'_, DesktopState>,
) -> Result<Value, AppError> {
    let mut query = Vec::new();
    push_query(&mut query, "startEpochDay", start_epoch_day);
    push_query(&mut query, "competitionId", competition_id);
    txline::api::authenticated_get(&state.client, &state.config, "api/fixtures/snapshot", query)
        .await
}

#[tauri::command]
pub async fn txline_odds_snapshot(
    fixture_id: u64,
    as_of: Option<u64>,
    state: State<'_, DesktopState>,
) -> Result<Value, AppError> {
    let mut query = Vec::new();
    push_query(&mut query, "asOf", as_of);
    txline::api::authenticated_get(
        &state.client,
        &state.config,
        &format!("api/odds/snapshot/{fixture_id}"),
        query,
    )
    .await
}

#[tauri::command]
pub async fn txline_odds_updates(
    fixture_id: u64,
    state: State<'_, DesktopState>,
) -> Result<Value, AppError> {
    txline::api::authenticated_get(
        &state.client,
        &state.config,
        &format!("api/odds/updates/{fixture_id}"),
        vec![],
    )
    .await
}

#[tauri::command]
pub async fn txline_odds_interval(
    epoch_day: u64,
    hour_of_day: u64,
    interval: u64,
    state: State<'_, DesktopState>,
) -> Result<Value, AppError> {
    txline::api::authenticated_get(
        &state.client,
        &state.config,
        &format!("api/odds/updates/{epoch_day}/{hour_of_day}/{interval}"),
        vec![],
    )
    .await
}

#[tauri::command]
pub async fn txline_scores_snapshot(
    fixture_id: u64,
    as_of: Option<u64>,
    state: State<'_, DesktopState>,
) -> Result<Value, AppError> {
    let mut query = Vec::new();
    push_query(&mut query, "asOf", as_of);
    txline::api::authenticated_get(
        &state.client,
        &state.config,
        &format!("api/scores/snapshot/{fixture_id}"),
        query,
    )
    .await
}

#[tauri::command]
pub async fn txline_scores_updates(
    fixture_id: u64,
    state: State<'_, DesktopState>,
) -> Result<Value, AppError> {
    txline::api::authenticated_get(
        &state.client,
        &state.config,
        &format!("api/scores/updates/{fixture_id}"),
        vec![],
    )
    .await
}

#[tauri::command]
pub async fn txline_scores_historical(
    fixture_id: u64,
    state: State<'_, DesktopState>,
) -> Result<Value, AppError> {
    txline::api::authenticated_get(
        &state.client,
        &state.config,
        &format!("api/scores/historical/{fixture_id}"),
        vec![],
    )
    .await
}

#[tauri::command]
pub async fn txline_scores_interval(
    epoch_day: u64,
    hour_of_day: u64,
    interval: u64,
    state: State<'_, DesktopState>,
) -> Result<Value, AppError> {
    txline::api::authenticated_get(
        &state.client,
        &state.config,
        &format!("api/scores/updates/{epoch_day}/{hour_of_day}/{interval}"),
        vec![],
    )
    .await
}

#[tauri::command]
pub async fn txline_scores_stat_validation(
    fixture_id: u64,
    seq: u64,
    stat_key: Option<u64>,
    stat_key2: Option<u64>,
    stat_keys: Option<String>,
    state: State<'_, DesktopState>,
) -> Result<Value, AppError> {
    let mut query = vec![
        ("fixtureId", fixture_id.to_string()),
        ("seq", seq.to_string()),
    ];
    push_query(&mut query, "statKey", stat_key);
    push_query(&mut query, "statKey2", stat_key2);
    if let Some(stat_keys) = stat_keys.filter(|value| !value.trim().is_empty()) {
        query.push(("statKeys", stat_keys));
    }
    txline::api::authenticated_get(
        &state.client,
        &state.config,
        "api/scores/stat-validation",
        query,
    )
    .await
}

#[tauri::command]
pub async fn fetch_txline(path: String, state: State<'_, DesktopState>) -> Result<Value, AppError> {
    // Escape hatch for backend-owned TxLINE reads. It is intentionally
    // allowlisted to documented GET data/proof endpoints; streams use
    // start_txline, and auth/token activation cannot be reached from here.
    txline::api::authenticated_get(&state.client, &state.config, &path, vec![]).await
}

fn push_query(query: &mut Vec<(&'static str, String)>, name: &'static str, value: Option<u64>) {
    if let Some(value) = value {
        query.push((name, value.to_string()));
    }
}
