//! Coral transcript commands.

use tauri::State;

use crate::error::AppError;
use crate::services::coralos::transcript;
use crate::state::DesktopState;
use crate::types::{AgentTraceEvent, CoralMessage};

#[tauri::command]
pub fn coral_list_messages(
    run_id: String,
    state: State<'_, DesktopState>,
) -> Result<Vec<CoralMessage>, AppError> {
    transcript::read_coral_messages(&state.replay_dir, &run_id)
}

#[tauri::command]
pub fn agent_list_trace(
    run_id: String,
    state: State<'_, DesktopState>,
) -> Result<Vec<AgentTraceEvent>, AppError> {
    transcript::read_agent_trace(&state.replay_dir, &run_id)
}
