//! Tauri command layer: thin IPC adapters over services and engines.
//!
//! Commands validate input, borrow `DesktopState`, delegate to a service, and
//! emit typed events via `crate::event_bus`. Business logic does not live
//! here — a command that grows beyond glue belongs in a service or engine.
//!
//! - `config`: redacted public configuration.
//! - `txline`: ingest lifecycle plus the documented TxLINE data endpoints.
//! - `chain`: allowlisted Solana RPC, chain status, and Yellowstone watches.
//! - `intelligence`: agent-round execution and run history.
//! - `coral`: replayable Coral transcript and agent trace reads.
//! - `arena`: arena positions, signals, settlement records, safety gates.
//! - `auth`: wallet-identity registration and profile lookup (sled-backed).
//! - `payments`: Solana Pay payment intent create/verify/list.

pub mod arena;
pub mod auth;
pub mod chain;
pub mod config;
pub mod coral;
pub mod intelligence;
pub mod payments;
pub mod txline;
