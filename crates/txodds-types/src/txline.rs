//! TxLINE feed types: events, odds quotes, scores, and proof receipts.
//!
//! All shapes here mirror the SSE payloads produced by the TxLINE API and are
//! shared between the live-ingest path and the persisted ledger receipts so both
//! drive the same UI and market engine without translation.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Normalized TxLINE event kinds consumed by the market engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TxLineEventKind {
    Fixture,
    ScoreUpdate,
    OddsUpdate,
    Goal,
    RedCard,
    FinalWhistle,
    OddsMove,
    ProofReceived,
}

/// Odds quote normalized from TxLINE odds payloads.
///
/// Both decimal odds and implied probability are stored because strategy code
/// reasons about probability movement, not only displayed prices.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OddsQuote {
    pub fixture_id: u64,
    pub outcome: String,
    pub decimal: f64,
    pub implied_probability: f64,
    pub source: Option<String>,
    pub ts: String,
}

/// Score tuple shown in event/delivery payloads.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Score {
    pub home: i64,
    pub away: i64,
}

/// Proof-validation simulation result attached to a [`TxLineProofReceipt`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationSimulationStatus {
    #[default]
    NotStarted,
    Passed,
    Failed,
    Unavailable,
}

/// Optional on-chain proof receipt attached to a TxLINE event.
///
/// Present when TxLINE or the txoracle program provides a verifiable
/// stat/proof reference suitable for settlement gating.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxLineProofReceipt {
    pub fixture_id: u64,
    #[serde(default)]
    pub seq: Option<u64>,
    #[serde(default)]
    pub stat_key: Option<u64>,
    #[serde(default)]
    pub stat_keys: Vec<String>,
    #[serde(default)]
    pub txline_ts: Option<String>,
    #[serde(default)]
    pub epoch_day: Option<u32>,
    #[serde(default)]
    pub merkle_root: Option<String>,
    #[serde(default)]
    pub stat_proof_hash: Option<String>,
    #[serde(default)]
    pub root_pda: Option<String>,
    #[serde(default)]
    pub txline_program: Option<String>,
    #[serde(default)]
    pub root_observed_slot: Option<u64>,
    #[serde(default)]
    pub proof_present: bool,
    #[serde(default)]
    pub root_present: bool,
    #[serde(default)]
    pub simulation_status: ValidationSimulationStatus,
    pub verified: bool,
    pub note: String,
    #[serde(default)]
    pub raw: Option<Value>,
}

/// Canonical TxLINE event shape across live ingestion and persisted receipts.
///
/// The `raw` field is preserved for debugging while all normalized fields drive
/// app behavior so callers never parse raw payloads twice.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxLineEvent {
    pub id: String,
    pub kind: TxLineEventKind,
    pub fixture_id: u64,
    #[serde(default)]
    pub seq: Option<u64>,
    #[serde(default)]
    pub txline_ts: Option<String>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub confirmed: Option<bool>,
    #[serde(default)]
    pub participant: Option<String>,
    #[serde(default)]
    pub period: Option<String>,
    #[serde(default)]
    pub stat_keys: Vec<String>,
    #[serde(default)]
    pub schema_family: Option<String>,
    pub title: String,
    pub body: String,
    pub ts: String,
    pub raw: Option<Value>,
    pub odds: Option<Vec<OddsQuote>>,
    pub score: Option<Score>,
    pub proof: Option<TxLineProofReceipt>,
}
