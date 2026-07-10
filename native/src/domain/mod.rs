//! Deterministic domain contracts for the agent engine.
//!
//! These types are the Rust side of the shared event contract; the TypeScript
//! mirrors live under `ui/core/agent/types.ts`. They are staged ahead of their
//! engines so every PR builds against a reviewed contract instead of inventing
//! shapes inline.
//!
//! Shared TxLINE/run types remain in `crate::types` because live ingest and
//! the legacy round engine already depend on them.

pub mod agent;
pub mod arena;
pub mod proof;
