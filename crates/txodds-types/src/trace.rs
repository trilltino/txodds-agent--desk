//! Agent trace types for structured per-step observability.
//!
//! Every step an agent takes during a round is emitted as an [`AgentTraceEvent`]
//! keyed by `run_id` so the UI trace panel can reconstruct the full execution
//! sequence for post-round review.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Phase discriminant for an individual agent execution step.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentTracePhase {
    Observe,
    Derive,
    ToolCall,
    ToolResult,
    LlmReasoning,
    Decision,
    Action,
    Proof,
    Payment,
    Evaluation,
}

/// A single structured trace step emitted by an agent during a round.
///
/// Steps are keyed by `(run_id, id)` so the UI can deduplicate live pushes
/// against the historical batch loaded on startup.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentTraceEvent {
    pub id: String,
    pub run_id: String,
    pub round: u64,
    pub phase: AgentTracePhase,
    pub summary: String,
    #[serde(default)]
    pub payload: Option<Value>,
    pub ts: String,
}
