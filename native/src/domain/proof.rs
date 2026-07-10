//! Proof contract: human-readable receipts over code-only verification.
//!
//! Every field that gates money or market state is computed deterministically;
//! `human_summary` is the only field an LLM may ever write.

#![allow(dead_code)] // Staged contract: consumed by the proof gate in PR 4.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnchainValidationStatus {
    NotStarted,
    SimulatedPass,
    SimulatedFail,
    TxPass,
    TxFail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationReceipt {
    pub id: String,
    pub fixture_id: u64,
    pub market_id: Option<String>,
    pub seq: Option<u32>,
    /// The evaluated predicate, verbatim, so receipts are auditable.
    pub predicate: String,
    pub txline_validation_fetched: bool,
    pub merkle_proof_present: bool,
    pub deterministic_predicate_passed: bool,
    pub onchain_validation_status: OnchainValidationStatus,
    pub tx_signature: Option<String>,
    pub explorer_url: Option<String>,
    pub human_summary: String,
    pub raw: Option<Value>,
}
