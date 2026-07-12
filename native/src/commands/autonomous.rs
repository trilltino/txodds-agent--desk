//! Autonomous-loop toggle commands. See `services::autonomous` for the loop
//! itself — these two commands only flip/read the runtime switch.

use std::sync::atomic::Ordering;

use tauri::State;

use crate::error::AppError;
use crate::state::DesktopState;

/// Enable or disable the autonomous live-trigger loop without a restart.
#[tauri::command]
pub async fn set_autonomous_loop_enabled(
    enabled: bool,
    state: State<'_, DesktopState>,
) -> Result<(), AppError> {
    state.autonomous_enabled.store(enabled, Ordering::Relaxed);
    Ok(())
}

/// Current state of the autonomous loop toggle.
#[tauri::command]
pub async fn get_autonomous_loop_enabled(state: State<'_, DesktopState>) -> Result<bool, AppError> {
    Ok(state.autonomous_enabled.load(Ordering::Relaxed))
}
