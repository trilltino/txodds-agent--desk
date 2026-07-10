//! Error type used by Tauri commands.
//!
//! Tauri serializes command errors to JavaScript, so AppError implements
//! `Serialize` manually to expose stable `{ code, message }` objects.

use serde::ser::{SerializeStruct, Serializer};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    // Configuration is missing, malformed, or intentionally disabled.
    #[error("configuration error: {0}")]
    Config(String),
    // User/webview input failed validation before any privileged operation.
    #[error("invalid input: {0}")]
    InvalidInput(String),
    // Requested ledger/runtime item does not exist.
    #[error("not found: {0}")]
    NotFound(String),
    // Solana JSON-RPC returned an application-level error object.
    #[error("rpc error {code}: {message}")]
    Rpc { code: i64, message: String },
    // Transport-level HTTP failure from reqwest.
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    // Filesystem or process IO failure.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    // SQLite ledger failure.
    #[error("sqlite error: {0}")]
    Sql(#[from] rusqlite::Error),
    // JSON parse/serialization failure.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    // Background task, sidecar, or timeout failure that does not fit another
    // typed category.
    #[error("task error: {0}")]
    Task(String),
    // Mutex guard was poisoned by a panicking writer.
    #[error("internal lock poisoned")]
    LockPoisoned,
}

impl AppError {
    // Short stable code used by the webview for branching/error copy.
    fn code(&self) -> &'static str {
        match self {
            Self::Config(_) => "CONFIG",
            Self::InvalidInput(_) => "INVALID_INPUT",
            Self::NotFound(_) => "NOT_FOUND",
            Self::Rpc { .. } => "RPC",
            Self::Http(_) => "HTTP",
            Self::Io(_) => "IO",
            Self::Sql(_) => "SQLITE",
            Self::Json(_) => "JSON",
            Self::Task(_) => "TASK",
            Self::LockPoisoned => "LOCK_POISONED",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Do not serialize internal error fields like backtraces or nested
        // reqwest/sqlite structures. The UI only needs code + display message.
        let mut state = serializer.serialize_struct("AppError", 2)?;
        state.serialize_field("code", self.code())?;
        state.serialize_field("message", &self.to_string())?;
        state.end()
    }
}
