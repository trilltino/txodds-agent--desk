//! Auth IPC commands — wallet-identity registration and profile lookup.
//!
//! Authentication is off-chain: the wallet public key is the user's identity.
//! No passwords, no JWTs.  A profile pairs a base58 public key with a chosen
//! display name and is persisted locally in the `sled` user store.
//!
//! ## Challenge / response flow (new registrations)
//!
//! 1. Frontend calls `issue_auth_challenge(public_key)` — backend stores a
//!    UUID nonce keyed on the public key and returns an `AuthChallenge`.
//! 2. Frontend passes `challenge.message` (UTF-8) to `window.solana.signMessage()`.
//! 3. Frontend calls `request_auth(public_key, signature, nonce, username?, cluster?)`
//!    — backend verifies the 64-byte Ed25519 signature, consumes the nonce, and
//!    upserts (or looks up) the profile.
//!
//! ## Commands
//!
//! | Command                | JS call                                | Effect |
//! |------------------------|----------------------------------------|--------|
//! | `issue_auth_challenge` | `invoke("issue_auth_challenge", …)`    | Generate a one-time sign challenge. |
//! | `request_auth`         | `invoke("request_auth", …)`            | Verify signature; return/create profile. |
//! | `get_user_profile`     | `invoke("get_user_profile", …)`        | Look up a profile by public key. |
//! | `save_user_profile`    | `invoke("save_user_profile", …)`       | Create or overwrite a profile. |
//! | `delete_user_profile`  | `invoke("delete_user_profile", …)`     | Remove a profile permanently. |

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use tauri::{Emitter, State};
use txodds_types::{AuthChallenge, UserProfile};

use crate::error::AppError;
use crate::state::DesktopState;
use crate::types::now_iso;

/// Maximum number of in-flight auth challenges before the oldest entries are
/// evicted.  This prevents unbounded memory growth from repeated
/// `issue_auth_challenge` calls that are never consumed by `request_auth`.
const MAX_PENDING_CHALLENGES: usize = 64;

// ── issue_auth_challenge ───────────────────────────────────────────────────────

/// Issue a one-time sign challenge for `public_key`.
///
/// The backend generates a UUID v4 nonce, stores it in `pending_challenges`
/// (keyed by nonce → (public_key, message)), and returns an `AuthChallenge`
/// for the wallet to sign.  The nonce is single-use: `request_auth` removes it
/// on first verification so it cannot be replayed.
#[tauri::command]
pub async fn issue_auth_challenge(
    public_key: String,
    state: State<'_, DesktopState>,
) -> Result<AuthChallenge, AppError> {
    if public_key.trim().is_empty() {
        return Err(AppError::InvalidInput("public_key must not be empty".into()));
    }
    let nonce = uuid::Uuid::new_v4().to_string();
    let ts = now_iso();
    let message = format!(
        "Sign to authenticate with TxOdds Agent Desk\nNonce: {nonce}\nIssued: {ts}"
    );
    let mut challenges = state
        .pending_challenges
        .lock()
        .map_err(|_| AppError::LockPoisoned)?;

    // Evict oldest entries when the map exceeds the safety cap.
    // HashMap iteration order is arbitrary, which is fine — all stale nonces
    // are equally expendable.
    while challenges.len() >= MAX_PENDING_CHALLENGES {
        if let Some(oldest_key) = challenges.keys().next().cloned() {
            challenges.remove(&oldest_key);
        } else {
            break;
        }
    }

    challenges.insert(nonce.clone(), (public_key, message.clone()));
    Ok(AuthChallenge { nonce, message, ts })
}

// ── request_auth ───────────────────────────────────────────────────────────────

/// Verify a wallet signature and return (or create) the stored user profile.
///
/// Steps:
/// 1. Consume the pending nonce (replay protection — each nonce is single-use).
/// 2. Confirm the `public_key` matches the one that requested the challenge.
/// 3. Decode the base58 public key and verify the 64-byte Ed25519 signature
///    over the UTF-8 challenge message bytes.
/// 4a. If `username` is provided → upsert the profile and return it.
/// 4b. If `username` is absent → look up and return the existing profile.
///
/// Returns `AppError::InvalidInput` when verification fails or no profile
/// exists and no username was supplied.
#[tauri::command]
pub async fn request_auth(
    public_key: String,
    signature: Vec<u8>,
    nonce: String,
    username: Option<String>,
    cluster: Option<String>,
    state: State<'_, DesktopState>,
) -> Result<UserProfile, AppError> {
    // ── 1. consume nonce ──────────────────────────────────────────────────────
    let (stored_key, message) = {
        let mut challenges = state
            .pending_challenges
            .lock()
            .map_err(|_| AppError::LockPoisoned)?;
        challenges.remove(&nonce).ok_or_else(|| {
            AppError::InvalidInput(
                "Unknown or expired nonce; call issue_auth_challenge first.".into(),
            )
        })?
    };

    // ── 2. key ownership check ─────────────────────────────────────────────────
    if stored_key != public_key {
        return Err(AppError::InvalidInput(
            "public_key does not match the key that requested this challenge.".into(),
        ));
    }

    // ── 3. Ed25519 verification ────────────────────────────────────────────────
    let key_bytes = bs58::decode(&public_key)
        .into_vec()
        .map_err(|_| AppError::InvalidInput("public_key is not valid base58.".into()))?;
    let key_arr: [u8; 32] = key_bytes.as_slice().try_into().map_err(|_| {
        AppError::InvalidInput("public_key must decode to exactly 32 bytes.".into())
    })?;
    let verifying_key = VerifyingKey::from_bytes(&key_arr)
        .map_err(|e| AppError::InvalidInput(format!("Invalid Ed25519 public key: {e}")))?;

    let sig_arr: [u8; 64] = signature.as_slice().try_into().map_err(|_| {
        AppError::InvalidInput("signature must be exactly 64 bytes.".into())
    })?;
    let sig = Signature::from_bytes(&sig_arr);

    verifying_key
        .verify(message.as_bytes(), &sig)
        .map_err(|_| AppError::InvalidInput("Ed25519 signature verification failed.".into()))?;

    // ── 4. upsert or look up profile ───────────────────────────────────────────
    let store = state.user_store.lock().map_err(|_| AppError::LockPoisoned)?;
    if let Some(name) = username {
        let cluster_str = cluster.unwrap_or_else(|| "devnet".to_owned());
        if !matches!(cluster_str.as_str(), "devnet" | "mainnet-beta") {
            return Err(AppError::InvalidInput(format!(
                "unknown cluster '{cluster_str}'; expected devnet or mainnet-beta"
            )));
        }
        let trimmed = name.trim().to_owned();
        if trimmed.is_empty() {
            return Err(AppError::InvalidInput("username must not be empty".into()));
        }
        store.save(&public_key, &trimmed, &cluster_str)
    } else {
        store.get(&public_key)?.ok_or_else(|| {
            AppError::InvalidInput(
                "No profile found for this key; provide a username to register.".into(),
            )
        })
    }
}

// ── get_user_profile ──────────────────────────────────────────────────────────

/// Return the stored `UserProfile` for the given wallet public key.
///
/// Returns `null` (JS) / `None` (Rust) if the key has never been registered.
/// The webview uses this immediately after wallet connect to decide whether to
/// show the registration form or proceed directly to the app.
#[tauri::command]
pub async fn get_user_profile(
    public_key: String,
    state: State<'_, DesktopState>,
) -> Result<Option<UserProfile>, AppError> {
    if public_key.trim().is_empty() {
        return Err(AppError::InvalidInput("public_key must not be empty".into()));
    }
    let store = state.user_store.lock().map_err(|_| AppError::LockPoisoned)?;
    store.get(&public_key)
}

// ── save_user_profile ─────────────────────────────────────────────────────────

/// Create or overwrite the `UserProfile` for `public_key`.
///
/// `username` is trimmed; empty strings are rejected.  `cluster` must be one
/// of `"devnet"` or `"mainnet-beta"`.
#[tauri::command]
pub async fn save_user_profile(
    public_key: String,
    username: String,
    cluster: String,
    state: State<'_, DesktopState>,
) -> Result<UserProfile, AppError> {
    if public_key.trim().is_empty() {
        return Err(AppError::InvalidInput("public_key must not be empty".into()));
    }
    let trimmed = username.trim().to_owned();
    if trimmed.is_empty() {
        return Err(AppError::InvalidInput("username must not be empty".into()));
    }
    if !matches!(cluster.as_str(), "devnet" | "mainnet-beta") {
        return Err(AppError::InvalidInput(format!(
            "unknown cluster '{cluster}'; expected devnet or mainnet-beta"
        )));
    }
    let store = state.user_store.lock().map_err(|_| AppError::LockPoisoned)?;
    store.save(&public_key, &trimmed, &cluster)
}

// ── delete_user_profile ───────────────────────────────────────────────────────

/// Permanently remove the `UserProfile` for `public_key`.
///
/// No-op if the profile does not exist.  Returns `null` on success.
#[tauri::command]
pub async fn delete_user_profile(
    public_key: String,
    state: State<'_, DesktopState>,
) -> Result<(), AppError> {
    if public_key.trim().is_empty() {
        return Err(AppError::InvalidInput("public_key must not be empty".into()));
    }
    let store = state.user_store.lock().map_err(|_| AppError::LockPoisoned)?;
    store.delete(&public_key)
}

// ── open_phantom_popup ─────────────────────────────────────────────────────────

/// Embedded popup page — port placeholder `__PORT__` is substituted at runtime.
const POPUP_HTML: &str = include_str!("../../../ui/core/wallet/phantom-popup.html");

/// Launch a Chrome `--app` popup that loads the Phantom wallet extension.
///
/// Phantom cannot inject `window.solana` into Tauri's WebView, but it **can**
/// inject into a real Chrome (or Brave) window.  This command:
///
/// 1. Binds a single-use HTTP server on a random loopback port.
/// 2. Injects the port into the popup HTML so the page knows where to POST.
/// 3. Launches Chrome in `--app` mode pointing at that local URL.
/// 4. Waits up to 120 s for the page to `POST /pubkey` with `{ pubkey }`.
/// 5. Emits a `phantom_pubkey` Tauri event to the webview and shuts down.
///
/// Returns immediately after Chrome is launched — the pubkey arrives
/// asynchronously via the `phantom_pubkey` event.
///
/// Returns `AppError::Task` when Chrome / Brave cannot be found or launched.
#[tauri::command]
pub async fn open_phantom_popup(app: tauri::AppHandle) -> Result<(), AppError> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| AppError::Task(format!("popup server bind failed: {e}")))?;

    let port = listener
        .local_addr()
        .map_err(|e| AppError::Task(format!("popup server addr failed: {e}")))?
        .port();

    let chrome = find_chrome().ok_or_else(|| {
        AppError::Task(
            "Google Chrome (or Brave) was not found. \
             Please install Chrome to use the Phantom popup, \
             or paste your public key manually."
                .into(),
        )
    })?;

    // Launch Chrome before the server task so the window appears immediately.
    //
    // Phantom needs a real Chromium window to inject `window.solana`, but the
    // user should only ever see Phantom's own approval popup — never the empty
    // host window.  `spawn_hidden_chrome` launches the host window and then
    // hides it entirely so the Chromium launcher never appears on screen.
    let app_url = format!("--app=http://127.0.0.1:{port}/");
    spawn_hidden_chrome(&chrome, &app_url)
        .map_err(|e| AppError::Task(format!("failed to launch Chrome: {e}")))?;

    tokio::spawn(serve_popup(listener, port, app));

    Ok(())
}

/// Launch a Chrome/Brave `--app` window **without** leaving a host window
/// visible to the user.
///
/// Phantom cannot inject into Tauri's WebView, so a real Chromium window is
/// required for the extension to run.  We do not want that empty launcher
/// window competing with Phantom's own popup.
///
/// On Windows, launch-time window flags (size/position/`-WindowStyle`) are
/// **ignored when Chrome is already running**, because the `--app` URL is handed
/// to the existing browser process which then creates a fresh, normal window —
/// that is the stray window users used to see.  To handle that case reliably we
/// launch Chrome normally and then run a short-lived, hidden PowerShell watcher
/// that fully **hides** (not just minimizes) any top-level window titled
/// `TxOdds` (the popup page's `<title>`) the moment it appears, using
/// `ShowWindow(SW_HIDE)`.  This removes it from the screen *and* the taskbar so
/// the user never sees a stray window.  Phantom's own approval popup is a
/// separate window with a different title, so it is left fully visible.
///
/// On macOS / Linux the 1×1 off-screen trick works reliably.
///
/// Either way Phantom's OS-level approval popup still appears normally — the
/// user only ever interacts with the Phantom wallet, not the host window.
fn spawn_hidden_chrome(chrome: &std::path::Path, app_url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        // 1. Launch the Chrome/Brave --app window normally. If Chrome is already
        //    open, this new window is created by the existing process and will
        //    briefly appear before the watcher below hides it.
        std::process::Command::new(chrome)
            .arg(app_url)
            .arg("--no-first-run")
            .arg("--no-default-browser-check")
            .spawn()?;

        // 2. Spawn a hidden PowerShell watcher that hides the host window by
        //    title as soon as it appears (and keeps doing so for a few seconds
        //    in case Chrome is slow to paint). We write the script to a temp
        //    file to avoid brittle inline-quoting of the Win32 interop block.
        let script = r#"
Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public class TxWin {
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc cb, IntPtr p);
  [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  // SW_HIDE = 0 — fully removes the window from screen AND taskbar, unlike
  // SW_MINIMIZE (6) which only shrinks it to the taskbar and still flashes.
  public static void HideByTitle(string match){
    EnumWindows(delegate(IntPtr h, IntPtr p){
      if(!IsWindowVisible(h)) return true;
      StringBuilder sb = new StringBuilder(256);
      GetWindowText(h, sb, 256);
      if(sb.ToString().IndexOf(match, StringComparison.OrdinalIgnoreCase) >= 0){ ShowWindow(h, 0); }
      return true;
    }, IntPtr.Zero);
  }
}
"@
# Poll aggressively so the host window is hidden before it ever paints, then
# keep watching for a short while in case Chrome is slow to create the window.
$deadline = (Get-Date).AddSeconds(20)
while((Get-Date) -lt $deadline){
  [TxWin]::HideByTitle('TxOdds')
  Start-Sleep -Milliseconds 50
}
"#;
        let mut script_path = std::env::temp_dir();
        script_path.push("txodds_hide_phantom_host.ps1");
        std::fs::write(&script_path, script)?;

        std::process::Command::new("powershell")
            .arg("-NoProfile")
            .arg("-WindowStyle")
            .arg("Hidden")
            .arg("-ExecutionPolicy")
            .arg("Bypass")
            .arg("-File")
            .arg(&script_path)
            .spawn()?;
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    {
        std::process::Command::new(chrome)
            .arg(app_url)
            .arg("--window-size=1,1")
            .arg("--window-position=-32000,-32000")
            .arg("--no-first-run")
            .arg("--no-default-browser-check")
            .spawn()?;
        Ok(())
    }
}

// ── popup HTTP server ──────────────────────────────────────────────────────────

/// Shared state threaded through the two axum route handlers.
#[derive(Clone)]
struct PopupState {
    /// Fully resolved HTML (port already substituted).
    html: std::sync::Arc<String>,
    /// One-shot sender used to trigger graceful shutdown after a pubkey lands.
    shutdown: std::sync::Arc<std::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
    app: tauri::AppHandle,
}

async fn serve_popup(listener: tokio::net::TcpListener, port: u16, app: tauri::AppHandle) {
    let html = POPUP_HTML.replace("__PORT__", &port.to_string());
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();

    let state = PopupState {
        html: std::sync::Arc::new(html),
        shutdown: std::sync::Arc::new(std::sync::Mutex::new(Some(tx))),
        app,
    };

    let router = axum::Router::new()
        .route("/", axum::routing::get(popup_html_handler))
        .route("/pubkey", axum::routing::post(popup_pubkey_handler))
        .with_state(state);

    let _ = axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            tokio::select! {
                _ = rx => {}
                _ = tokio::time::sleep(std::time::Duration::from_secs(120)) => {}
            }
        })
        .await;
}

async fn popup_html_handler(
    axum::extract::State(s): axum::extract::State<PopupState>,
) -> axum::response::Html<String> {
    axum::response::Html(s.html.as_ref().clone())
}

#[derive(serde::Deserialize)]
struct PubkeyBody {
    pubkey: String,
}

async fn popup_pubkey_handler(
    axum::extract::State(s): axum::extract::State<PopupState>,
    axum::Json(body): axum::Json<PubkeyBody>,
) -> axum::http::StatusCode {
    let trimmed = body.pubkey.trim().to_owned();
    if !trimmed.is_empty() {
        let _ = s.app.emit("phantom_pubkey", trimmed);
    }
    // Close the server — only one pubkey is needed.
    if let Ok(mut guard) = s.shutdown.lock() {
        if let Some(tx) = guard.take() {
            let _ = tx.send(());
        }
    }
    axum::http::StatusCode::OK
}

// ── Chrome / Brave discovery ───────────────────────────────────────────────────

fn find_chrome() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    return {
        let candidates = [
            r"C:\Program Files\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe",
            r"C:\Program Files (x86)\BraveSoftware\Brave-Browser\Application\brave.exe",
        ];
        let mut found: Option<std::path::PathBuf> = None;
        for path in &candidates {
            let p = std::path::Path::new(path);
            if p.exists() {
                found = Some(p.to_path_buf());
                break;
            }
        }
        found
    };

    #[cfg(target_os = "macos")]
    return {
        let candidates = [
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
        ];
        let mut found: Option<std::path::PathBuf> = None;
        for path in &candidates {
            let p = std::path::Path::new(path);
            if p.exists() {
                found = Some(p.to_path_buf());
                break;
            }
        }
        found
    };

    #[cfg(target_os = "linux")]
    return {
        let names = [
            "google-chrome",
            "google-chrome-stable",
            "chromium-browser",
            "chromium",
            "brave-browser",
        ];
        let mut found: Option<std::path::PathBuf> = None;
        'outer: for name in &names {
            if let Some(path_var) = std::env::var_os("PATH") {
                for dir in std::env::split_paths(&path_var) {
                    let full = dir.join(name);
                    if full.exists() {
                        found = Some(full);
                        break 'outer;
                    }
                }
            }
        }
        found
    };

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    return None;
}
