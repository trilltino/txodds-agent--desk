 //! Managed desktop state and runtime path resolution.
//!
//! `DesktopState` is the one struct handed to `app.manage()`: shared clients,
//! supervised task handles, and durable directories. Command modules borrow it
//! through `tauri::State`; nothing here is serialized to the webview.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use reqwest::Client;

use crate::config::AppConfig;
use crate::services::chain::yellowstone::YellowstoneHandle;
use crate::services::ledger::LedgerStore;
use crate::services::proof::ValidationBridge;
use crate::services::user_store::UserStore;

pub struct DesktopState {
    /// Full config may contain secrets; only `PublicConfig` is returned to JS.
    pub config: AppConfig,
    /// Shared HTTP client for Triton, TxLINE, and sidecar-adjacent calls.
    pub client: Client,
    /// SQLite is protected by a Mutex because Tauri commands/background tasks
    /// can access it concurrently, while `rusqlite::Connection` is synchronous.
    pub ledger: Arc<Mutex<LedgerStore>>,
    /// Current TxLINE ingest task. Starting a new mode aborts the previous one.
    pub txline_task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    /// Optional Yellowstone supervisor; absent when gRPC config is missing.
    pub yellowstone: Option<YellowstoneHandle>,
    /// Read-only txoracle proof validation bridge.
    pub validation_bridge: ValidationBridge,
    /// App-data directories, not repo paths, for durable user/runtime output.
    pub replay_dir: PathBuf,
    pub export_dir: PathBuf,
    /// Local sled user profile store (public key → UserProfile).
    pub user_store: Mutex<UserStore>,
    /// In-flight auth nonces issued by `issue_auth_challenge`.
    /// Maps nonce (UUID v4) → (base58 public key, signed message text).
    /// Entries are consumed on first use so replay attacks are impossible.
    pub pending_challenges: Mutex<HashMap<String, (String, String)>>,
}

/// Locate a named sidecar script across dev, legacy, and packaged layouts.
pub fn resolve_named_sidecar_path(app: &tauri::App, name: &str) -> PathBuf {
    use tauri::Manager;

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    // Development layout after workspace compartmentalization.
    let dev_path = cwd.join("runtime").join("sidecars").join(name);
    if dev_path.exists() {
        return dev_path;
    }

    // Legacy fallback keeps older local builds/packages from breaking while the
    // repo finishes migrating away from root sidecars/.
    let legacy_dev_path = cwd.join("sidecars").join(name);
    if legacy_dev_path.exists() {
        return legacy_dev_path;
    }

    let resource_dir = app
        .path()
        .resource_dir()
        .unwrap_or_else(|_| PathBuf::from("."));
    // Packaged resource layout mirrors the source runtime/sidecars directory.
    let packaged_sidecar = resource_dir.join("runtime").join("sidecars").join(name);
    if packaged_sidecar.exists() {
        return packaged_sidecar;
    }

    // Legacy packaged resource fallback.
    let legacy_packaged_sidecar = resource_dir.join("sidecars").join(name);
    if legacy_packaged_sidecar.exists() {
        return legacy_packaged_sidecar;
    }

    resource_dir.join(name)
}

/// Locate a non-executable resource directory in dev or packaged layouts.
pub fn resolve_resource_dir(app: &tauri::App, relative: &[&str]) -> PathBuf {
    use tauri::Manager;

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut dev_path = cwd;
    for segment in relative {
        dev_path = dev_path.join(segment);
    }
    if dev_path.exists() {
        return dev_path;
    }

    let mut resource_path = app
        .path()
        .resource_dir()
        .unwrap_or_else(|_| PathBuf::from("."));
    for segment in relative {
        resource_path = resource_path.join(segment);
    }
    resource_path
}
