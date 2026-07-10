//! Browser/PWA wallet context mirror.
//!
//! The frontend owns wallet detection today; this type is staged here so the
//! Rust backend can mirror wallet state for settlement gating once the native
//! wallet adapter is wired up.

use serde::{Deserialize, Serialize};

/// Browser/PWA wallet context mirrored from the frontend wallet adapter.
///
/// `cluster` is a freeform string here (matching the JS adapter) rather than
/// the typed [`crate::cluster::Cluster`] so unknown network strings don't cause
/// a deserialization failure.
#[allow(dead_code)] // Browser/PWA wallet context mirror; frontend owns local detection today.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletContext {
    pub provider: String,
    pub public_key: Option<String>,
    pub connected: bool,
    pub cluster: String,
}
