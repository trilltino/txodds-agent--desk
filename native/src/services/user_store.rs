//! Local user profile store backed by `sled`.
//!
//! `sled` is an embedded key-value database: no daemon, no configuration, no
//! network.  Data lives in `$APP_DATA/txodds-agent-desk/user_store/`.
//!
//! ## Key schema
//!
//! ```
//! key:   UTF-8 base58 Solana public key
//! value: JSON-encoded [`UserProfile`]
//! ```
//!
//! The store is opened once at app startup and wrapped in `Arc<UserStore>` so
//! every Tauri command shares the same handle without re-opening the tree.

use std::path::Path;

use txodds_types::UserProfile;

use crate::error::AppError;
use crate::types::now_iso;

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
    pub fn delete(&self, public_key: &str) -> Result<(), AppError> {
        self.db
            .remove(public_key)
            .map_err(|e| AppError::Task(e.to_string()))?;
        self.db.flush().map_err(|e| AppError::Task(e.to_string()))?;
        Ok(())
    }
}
