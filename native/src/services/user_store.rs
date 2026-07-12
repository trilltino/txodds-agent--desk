//! Local user profile store backed by `sled`.
//!
//! `sled` is an embedded key-value database: no daemon, no configuration, no
//! network.  Data lives in `$APP_DATA/txodds-agent-desk/user_store/`.
//!
//! ## Key schema
//!
//! ```text
//! key:   UTF-8 base58 Solana public key
//! value: JSON-encoded UserProfile
//!
//! key:   "__last_session__"        (reserved — '_' is not in base58)
//! value: JSON-encoded SavedSession (remembered wallet for auto-login)
//! ```
//!
//! The store is opened once at app startup and wrapped in `Arc<UserStore>` so
//! every Tauri command shares the same handle without re-opening the tree.

use std::path::Path;

use serde::{Deserialize, Serialize};
use txodds_types::UserProfile;

use crate::error::AppError;
use crate::types::now_iso;

/// Reserved sled key for the remembered wallet session. Base58 excludes `_`,
/// so this can never collide with a profile key.
const SESSION_KEY: &str = "__last_session__";

/// Remembered wallet session written after a successful authentication so the
/// next launch can skip the connect flow. Holds no key material — the profile
/// is a local display name and every payment still requires a fresh wallet
/// signature, so restoring a session grants nothing spendable.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SavedSession {
    public_key: String,
    saved_at: String,
}

/// Thin wrapper around an open `sled` tree.
pub struct UserStore {
    db: sled::Db,
}

impl UserStore {
    /// Open (or create) the store at the given directory path.
    ///
    /// Called once during `app.setup()` with `app_data_dir/user_store`.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, AppError> {
        let db = sled::open(path).map_err(|e| AppError::Task(e.to_string()))?;
        Ok(Self { db })
    }

    // ── queries ───────────────────────────────────────────────────────────────

    /// Return the profile for `public_key`, or `None` if not registered.
    pub fn get(&self, public_key: &str) -> Result<Option<UserProfile>, AppError> {
        match self.db.get(public_key).map_err(|e| AppError::Task(e.to_string()))? {
            Some(bytes) => {
                let profile: UserProfile =
                    serde_json::from_slice(&bytes).map_err(AppError::Json)?;
                Ok(Some(profile))
            }
            None => Ok(None),
        }
    }

    // ── mutations ─────────────────────────────────────────────────────────────

    /// Upsert a profile.  `public_key` in the struct must match the DB key.
    pub fn save(&self, public_key: &str, username: &str, cluster: &str) -> Result<UserProfile, AppError> {
        let profile = UserProfile {
            public_key: public_key.to_owned(),
            username: username.trim().to_owned(),
            cluster: cluster.to_owned(),
            created_at: now_iso(),
        };
        let bytes = serde_json::to_vec(&profile).map_err(AppError::Json)?;
        self.db
            .insert(public_key, bytes)
            .map_err(|e| AppError::Task(e.to_string()))?;
        // Flush synchronously so data survives an immediate crash.
        self.db.flush().map_err(|e| AppError::Task(e.to_string()))?;
        Ok(profile)
    }

    /// Remove a profile.  No-op if the key does not exist.
    /// Also forgets the remembered session when it points at this key, so a
    /// deleted profile can never be silently restored on the next launch.
    pub fn delete(&self, public_key: &str) -> Result<(), AppError> {
        self.db
            .remove(public_key)
            .map_err(|e| AppError::Task(e.to_string()))?;
        if self.get_session()?.as_deref() == Some(public_key) {
            self.db
                .remove(SESSION_KEY)
                .map_err(|e| AppError::Task(e.to_string()))?;
        }
        self.db.flush().map_err(|e| AppError::Task(e.to_string()))?;
        Ok(())
    }

    // ── remembered session ────────────────────────────────────────────────────

    /// Remember `public_key` as the active wallet session for auto-login.
    pub fn save_session(&self, public_key: &str) -> Result<(), AppError> {
        let session = SavedSession {
            public_key: public_key.to_owned(),
            saved_at: now_iso(),
        };
        let bytes = serde_json::to_vec(&session).map_err(AppError::Json)?;
        self.db
            .insert(SESSION_KEY, bytes)
            .map_err(|e| AppError::Task(e.to_string()))?;
        self.db.flush().map_err(|e| AppError::Task(e.to_string()))?;
        Ok(())
    }

    /// Return the remembered session's public key, or `None`.
    pub fn get_session(&self) -> Result<Option<String>, AppError> {
        match self
            .db
            .get(SESSION_KEY)
            .map_err(|e| AppError::Task(e.to_string()))?
        {
            Some(bytes) => {
                let session: SavedSession =
                    serde_json::from_slice(&bytes).map_err(AppError::Json)?;
                Ok(Some(session.public_key))
            }
            None => Ok(None),
        }
    }

    /// Forget the remembered session (sign out).  No-op when none exists.
    pub fn clear_session(&self) -> Result<(), AppError> {
        self.db
            .remove(SESSION_KEY)
            .map_err(|e| AppError::Task(e.to_string()))?;
        self.db.flush().map_err(|e| AppError::Task(e.to_string()))?;
        Ok(())
    }
}
