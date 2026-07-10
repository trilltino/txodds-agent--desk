//! Shared backend data contracts.
//!
//! These structs are serialized through Tauri IPC/events and intentionally
//! mirror the frontend TypeScript contracts in `ui/types.ts`.
//!
//! This crate has zero Tauri, zero async — pure data.
//!
//! ## Module layout
//!
//! | Module    | Contents |
//! |-----------|----------|
//! | `cluster` | [`Cluster`], [`TrackMode`] |
//! | `txline`  | [`TxLineEvent`], [`TxLineEventKind`], [`OddsQuote`], [`Score`], [`TxLineProofReceipt`], [`ValidationSimulationStatus`] |
//! | `oracle`  | [`TxOracleRootEvent`], [`TxOracleInstructionKind`] |
//! | `agent`   | [`AgentRun`], [`AgentBid`], [`AgentDelivery`], [`AgentRole`], [`VerificationVerdict`], [`SettlementReceipt`], … |
//! | `chain`   | [`ChainStatus`], [`ChainObservation`], [`MarketRoundEvent`] |
//! | `coral`   | [`CoralMessage`], [`CoralSession`], [`CoralVerb`] |
//! | `trace`   | [`AgentTraceEvent`], [`AgentTracePhase`] |
//! | `wager`   | [`Wager`], [`WagerStatus`], [`Selection`], [`DebateSummary`], [`kelly_fraction`] |
//! | `wallet`  | [`WalletContext`] |
//! | `user`    | [`UserProfile`] |
//!
//! All types are re-exported flat from the crate root so existing callers
//! (`use txodds_types::SomeName`) continue to compile without changes.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used)]

pub mod agent;
pub mod chain;
pub mod cluster;
pub mod coral;
pub mod oracle;
pub mod trace;
pub mod txline;
pub mod user;
pub mod wager;
pub mod wallet;

pub use agent::*;
pub use chain::*;
pub use cluster::*;
pub use coral::*;
pub use oracle::*;
pub use trace::*;
pub use txline::*;
pub use user::*;
pub use wager::*;
pub use wallet::*;

/// Millisecond-precision UTC timestamp used across timeline and event payloads.
pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
