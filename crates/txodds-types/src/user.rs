//! User profile — public key + chosen username stored locally.
//!
//! This is the only user-identity type in the system.  No passwords, no JWTs.
//! The wallet public key is the user's identity; the username is cosmetic only.
//!
//! Stored in the `sled` embedded KV store under the key = base58 public key.

use serde::{Deserialize, Serialize};

// ── AuthChallenge ─────────────────────────────────────────────────────────────

/// One-time sign challenge issued by the backend before a new profile is saved.
///
/// The frontend encodes `message` as UTF-8 bytes, passes them to
/// `window.solana.signMessage()`, and returns the 64-byte Ed25519 signature
/// together with the `nonce` to `request_auth`.  The nonce is consumed on
/// first use so it cannot be replayed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthChallenge {
    /// UUID v4 used as a replay-prevention key; the frontend echoes it back.
    pub nonce: String,
    /// Human-readable UTF-8 string the wallet should sign.
    pub message: String,
    /// ISO-8601 timestamp of challenge issuance (UTC, ms precision).
    pub ts: String,
}

// ── UserProfile ───────────────────────────────────────────────────────────────

/// A locally-persisted user account bound to a Solana wallet public key.
///
/// Registration is off-chain: the user connects their wallet (public key is
/// derived from the wallet adapter) and picks a username.  No transaction or
/// signature is required.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserProfile {
    /// Base58-encoded Solana public key — the canonical identity token.
    pub public_key: String,
    /// Human-readable display name chosen at registration.
    pub username: String,
    /// Solana cluster the profile was created on ("devnet" | "mainnet-beta").
    pub cluster: String,
    /// ISO-8601 creation timestamp (millisecond precision, UTC).
    pub created_at: String,
}
