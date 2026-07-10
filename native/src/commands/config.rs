//! Configuration commands.

use tauri::State;

use crate::config::PublicConfig;
use crate::state::DesktopState;

#[tauri::command]
pub fn get_config(state: State<'_, DesktopState>) -> PublicConfig {
    // Never serialize AppConfig directly; it may contain tokens/keypaths.
    state.config.public()
}
