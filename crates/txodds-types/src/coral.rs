//! CoralOS multi-agent protocol types: sessions, messages, and verbs.
//!
//! Coral is the message-passing layer between agents in a round.  Every agent
//! thought, tool call, result, proof request, and payment event is represented
//! as a [`CoralMessage`] with a [`CoralVerb`] discriminant so the UI trace
//! panel can render the full agent dialogue.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::cluster::TrackMode;

/// Verb discriminant for every message in the Coral protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CoralVerb {
    Observed,
    Normalized,
    RootObserved,
    Want,
    AgentThought,
    ToolCall,
    ToolResult,
    Signal,
    ProofRequested,
    ProofReceived,
    ValidationSimulated,
    PaymentRequired,
    WalletConnected,
    PaymentProof,
    PaymentConfirmed,
    Verified,
    Settled,
    Evaluated,
}

/// Coral session — one per fixture × track combination.
///
/// All [`CoralMessage`]s in a round share the same `session_id` and
/// `thread_id`, giving the trace panel a stable grouping key.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoralSession {
    pub id: String,
    pub thread_id: String,
    pub fixture_id: u64,
    pub track: TrackMode,
    pub created_at: String,
}

/// A single Coral protocol message exchanged between agents in a round.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoralMessage {
    pub id: String,
    pub session_id: String,
    pub thread_id: String,
    pub round: u64,
    pub from: String,
    #[serde(default)]
    pub to: Vec<String>,
    pub verb: CoralVerb,
    pub text: String,
    #[serde(default)]
    pub payload: Option<Value>,
    pub ts: String,
}
