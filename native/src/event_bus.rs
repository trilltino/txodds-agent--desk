//! Typed native-event names: the single source of truth for `app.emit` topics.
//!
//! React subscribes to these exact strings through `src/desktop/events.ts`;
//! keeping both files as mirrored constant tables prevents topic-name drift
//! across the IPC boundary. Emit payload types live in `types.rs` so the wire
//! contract stays reviewable in one place.
//!
//! Reserved (not yet emitted) topics for the lean-track engines are listed at
//! the bottom; add the emitting service and the TypeScript constant in the same
//! change that first publishes them.

/// Normalized TxLINE event from live SSE ingest.
pub const TXLINE_EVENT: &str = "txline://event";
/// Connection health for TxLINE ingest and the Yellowstone supervisor.
pub const INGEST_STATUS: &str = "ingest://status";
/// Chain heartbeat from Triton RPC polling or Yellowstone slot pushes.
pub const CHAIN_SLOT: &str = "chain://slot";
/// Watched-account updates streamed by the Yellowstone sidecar.
pub const CHAIN_ACCOUNT: &str = "chain://account";
/// Watched-program/reference transactions streamed by the Yellowstone sidecar.
pub const CHAIN_TX: &str = "chain://tx";
/// Structured txoracle root publication decoded from a watched transaction.
#[allow(dead_code)] // Emitted once txoracle decoder lands; mirrored in TypeScript now.
pub const TXORACLE_ROOT: &str = "chain://txoracle-root";
/// Coral orchestration message for the active run/session transcript.
pub const CORAL_MESSAGE: &str = "coral://message";
/// Coral session lifecycle event.
pub const CORAL_SESSION: &str = "coral://session";
/// Match Intelligence Agent trace entry.
pub const AGENT_TRACE: &str = "agent://trace";
/// Match Intelligence Agent signal event.
pub const AGENT_SIGNAL: &str = "agent://signal";
/// Match Intelligence Agent evaluation event.
pub const AGENT_EVALUATION: &str = "agent://evaluation";
/// TxLINE proof receipt visible to Web3/proof UI.
pub const WEB3_PROOF_RECEIPT: &str = "web3://proof-receipt";
/// Validation simulation status visible to Web3/proof UI.
pub const VALIDATION_STATUS: &str = "web3://validation-status";
/// A Solana Pay transfer request was created for a run.
pub const PAY_INTENT: &str = "pay://intent";
/// A Solana Pay reference changed verification status.
pub const PAY_STATUS: &str = "pay://status";
/// Settlement receipt attached to a run (CoralOS, Solana Pay, or observation).
pub const SETTLE_RECEIPT: &str = "settle://receipt";
/// Phase-by-phase round timeline replay for UI animation.
pub const MARKET_ROUND: &str = "market://round";
/// App-internal notification payloads.
pub const APP_NOTIFICATION: &str = "app://notification";
/// Browser/PWA wallet status payloads.
#[allow(dead_code)] // Browser/PWA wallet status is local-first until backend binding lands.
pub const WALLET_STATUS: &str = "wallet://status";
