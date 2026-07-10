//! Chain commands: allowlisted Solana RPC, status heartbeat, and Yellowstone
//! watch registration. All RPC credentials stay in Rust.

use serde_json::Value;
use tauri::{AppHandle, Emitter, State};

use crate::error::AppError;
use crate::event_bus;
use crate::services::chain;
use crate::state::DesktopState;
use crate::types::{ChainObservation, ChainStatus, Cluster};

#[tauri::command]
pub async fn chain_rpc(
    cluster: Cluster,
    method: String,
    params: Option<Value>,
    state: State<'_, DesktopState>,
) -> Result<Value, AppError> {
    // chain::rpc validates the method allowlist before any network call.
    chain::rpc::solana_rpc(
        &state.client,
        &state.config,
        cluster,
        &method,
        params.unwrap_or(Value::Array(vec![])),
    )
    .await
}

#[tauri::command]
pub async fn chain_status(
    cluster: Cluster,
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<ChainStatus, AppError> {
    let status = chain::rpc::chain_status(&state.client, &state.config, cluster).await?;
    // Echo status as an event so UI subscribers and one-shot command callers see
    // consistent chain health.
    let _ = app.emit(event_bus::CHAIN_SLOT, &status);
    Ok(status)
}

#[tauri::command]
pub async fn observe_settlement(
    reference: String,
    escrow_account: Option<String>,
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<ChainObservation, AppError> {
    // Register continuous watches first when Yellowstone is available, then take
    // an immediate RPC snapshot for command response/persistence.
    if let Some(yellowstone) = &state.yellowstone {
        yellowstone.watch_reference(reference.clone());
        if let Some(account) = escrow_account.clone() {
            yellowstone.watch_account(account);
        }
    }
    let observation =
        chain::rpc::observe_settlement(&state.client, &state.config, reference, escrow_account)
            .await?;
    let _ = app.emit(event_bus::SETTLE_RECEIPT, &observation);
    Ok(observation)
}

#[tauri::command]
pub fn watch_account(account: String, state: State<'_, DesktopState>) -> Result<(), AppError> {
    // Watch commands are thin IPC adapters around the Yellowstone supervisor.
    let yellowstone = state
        .yellowstone
        .as_ref()
        .ok_or_else(|| AppError::Config("Yellowstone is not configured".to_string()))?;
    yellowstone.watch_account(account);
    Ok(())
}

#[tauri::command]
pub fn watch_program(program_id: String, state: State<'_, DesktopState>) -> Result<(), AppError> {
    let yellowstone = state
        .yellowstone
        .as_ref()
        .ok_or_else(|| AppError::Config("Yellowstone is not configured".to_string()))?;
    yellowstone.watch_program(program_id);
    Ok(())
}

#[tauri::command]
pub fn watch_reference(reference: String, state: State<'_, DesktopState>) -> Result<(), AppError> {
    let yellowstone = state
        .yellowstone
        .as_ref()
        .ok_or_else(|| AppError::Config("Yellowstone is not configured".to_string()))?;
    yellowstone.watch_reference(reference);
    Ok(())
}

#[tauri::command]
pub async fn yellowstone_status(state: State<'_, DesktopState>) -> Result<String, AppError> {
    // Explicit status command helps distinguish "not configured" from "running".
    if state.yellowstone.is_some() {
        Ok("Yellowstone gRPC observer is running".to_string())
    } else {
        chain::rpc::yellowstone_status(&state.config).await
    }
}
