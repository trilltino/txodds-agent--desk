//! Chain health, observation, and market round event types.
//!
//! These shapes are emitted by the Solana chain-watcher sidecar and consumed
//! by the UI chain-status ribbon and the market proof panel.

use serde::{Deserialize, Serialize};

use crate::cluster::Cluster;

/// Chain health/status emitted as the `chain://slot` event.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChainStatus {
    pub cluster: Cluster,
    pub slot: u64,
    pub solana_core: String,
    pub latency_ms: u128,
    pub ts: String,
}

/// Snapshot observation for a settlement reference, account, or program.
///
/// Emitted by the chain-watcher whenever it verifies a reference used in a
/// market round (e.g. escrow PDA funded, release tx confirmed).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChainObservation {
    pub kind: String,
    pub signature: Option<String>,
    pub slot: Option<u64>,
    pub blockhash: Option<String>,
    pub account: Option<String>,
    pub program_id: Option<String>,
    pub note: String,
}

/// Event payload emitted for each market phase transition.
///
/// Published on the `market://round` channel so the UI timeline can render
/// phase badges without polling the SQLite ledger.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketRoundEvent {
    pub run_id: String,
    pub phase: String,
    pub detail: String,
    pub at: String,
}
