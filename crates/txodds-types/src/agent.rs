//! Agent lifecycle types: roles, bids, deliveries, verdicts, settlement, and runs.
//!
//! A single market round flows through: bid → winner → delivery → verdict → settlement.
//! Every step is recorded in [`AgentRun`] which is the unit persisted to SQLite.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::cluster::TrackMode;
use crate::txline::TxLineEvent;

/// Coral market role used for scoring and track filtering.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentRole {
    Sharp,
    Risk,
    Pundit,
    Settlement,
    Fan,
    Verifier,
}

/// Bid submitted by a seller/verifier/settlement agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentBid {
    pub agent_id: String,
    pub role: AgentRole,
    pub price_sol: f64,
    pub confidence: f64,
    pub eta_ms: u64,
    pub note: String,
}

/// Hash-bound artifact produced by the winning agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDelivery {
    pub agent_id: String,
    pub title: String,
    pub payload: String,
    pub sha256: String,
    pub citations: Vec<String>,
    pub strategy: Option<String>,
    pub risk: Option<String>,
    pub fan_copy: Option<String>,
}

/// Verifier decision state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerdictStatus {
    Pass,
    Fail,
    NeedsReview,
}

/// Individual checks performed by the verifier.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VerdictCheck {
    TxlineInput,
    Hash,
    Proof,
    Policy,
    Settlement,
}

/// Structured verifier result used to gate settlement.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationVerdict {
    pub status: VerdictStatus,
    pub reason: String,
    pub checked: Vec<VerdictCheck>,
}

/// Settlement lifecycle state shown in the UI and persisted in the ledger.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettlementStatus {
    NotStarted,
    EscrowCreated,
    Deposited,
    Released,
    Refunded,
}

/// Settlement receipt from Solana Pay, CoralOS sidecar, or future native escrow.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettlementReceipt {
    pub rail: Option<String>,
    pub status: SettlementStatus,
    pub reference: Option<String>,
    pub escrow_pda: Option<String>,
    pub deposit_tx: Option<String>,
    pub release_tx: Option<String>,
    pub explorer_url: Option<String>,
    pub chain_observed: Option<bool>,
    pub chain_slot: Option<u64>,
    pub payment_url: Option<String>,
    pub payment_reference: Option<String>,
    pub payment_memo: Option<String>,
    pub payment_signature: Option<String>,
    pub payment_status: Option<String>,
    pub payment_recipient: Option<String>,
    pub payment_amount_sol: Option<f64>,
}

/// Timeline entry for the proof/audit panel.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineEntry {
    pub at: String,
    pub label: String,
    pub detail: String,
}

/// Full market round persisted to SQLite.
///
/// Carries the complete lifecycle from triggering event through settlement,
/// including every agent bid, the winning bid, the delivered artifact, the
/// verifier verdict, and the on-chain settlement receipt.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRun {
    pub run_id: String,
    pub track: TrackMode,
    pub trigger: TxLineEvent,
    pub bids: Vec<AgentBid>,
    pub winner: Option<AgentBid>,
    pub delivery: Option<AgentDelivery>,
    pub verdict: Option<VerificationVerdict>,
    pub settlement: Option<SettlementReceipt>,
    pub timeline: Vec<TimelineEntry>,
}

/// Arbitrary JSON payload attached to an agent step — kept for forward
/// compatibility so new fields added by agents don't break older clients.
pub type AgentPayload = Value;
