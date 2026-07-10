//! TxOracle on-chain root publication types.
//!
//! These types are staged for the txoracle decoder that decodes Yellowstone
//! transaction events into structured root publications. The wire contract is
//! mirrored in `ui/desktop/events.ts` as `chain://txoracle-root`.

use serde::{Deserialize, Serialize};

/// Instruction kinds published by the txoracle program.
#[allow(dead_code)] // Staged txoracle decoder wire contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TxOracleInstructionKind {
    InsertScoresRoot,
    InsertBatchRoot,
    InsertFixturesRoot,
    Unknown,
}

/// Decoded txoracle root event observed from a Yellowstone-watched transaction.
#[allow(dead_code)] // Staged txoracle decoder wire contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxOracleRootEvent {
    pub signature: String,
    pub slot: u64,
    pub program_id: String,
    pub instruction: TxOracleInstructionKind,
    pub epoch_day: Option<u32>,
    pub merkle_root: Option<String>,
    pub root_pda: Option<String>,
    pub fixture_id: Option<u64>,
}
