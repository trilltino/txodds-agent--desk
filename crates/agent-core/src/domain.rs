//! Agent signal/decision domain types and the proof-gate decision struct.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ── Proof gate ───────────────────────────────────────────────────────────────

/// Result of the deterministic proof gate check. Lives here so agent-core has
/// no dependency on the proof service crate, which in turn avoids a cycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProofGateDecision {
    /// Whether the proof gate check passed.
    pub pass: bool,
    /// Human-readable reason for the pass/fail outcome.
    pub reason: String,
    /// Names of the individual checks that were evaluated.
    pub checked: Vec<String>,
}

// ── Signals ──────────────────────────────────────────────────────────────────

/// Category of market signal emitted by the intelligence pipeline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SignalType {
    /// A score change (e.g. a goal) occurred in the match.
    ScoreEvent,
    /// A red card that can reprice the market.
    RedCardReprice,
    /// A sharp (informed) odds movement was detected.
    SharpOddsMove,
    /// A proof receipt became available for verification.
    ProofReady,
    /// A general context update with no dedicated category.
    ContextUpdate,
}

/// Relative importance of a signal, used to prioritise handling.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalSeverity {
    /// Requires immediate attention.
    Critical,
    /// High importance.
    High,
    /// Medium importance.
    Medium,
    /// Low importance / informational.
    Low,
}

/// A market signal derived from an incoming `TxLine` event.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSignal {
    /// Unique identifier for this signal.
    pub id: String,
    /// Fixture (match) the signal relates to.
    pub fixture_id: u64,
    /// Identifier of the source event that produced this signal.
    pub source_event_id: String,
    /// Category of the signal.
    pub signal_type: SignalType,
    /// Severity/priority of the signal.
    pub severity: SignalSeverity,
    /// Confidence score in the range `[0.0, 1.0]`.
    pub confidence: f64,
    /// Feature values extracted from the source event.
    pub features: BTreeMap<String, serde_json::Value>,
    /// Human-readable rationale explaining why the signal fired.
    pub rationale: String,
    /// ISO-8601 timestamp of when the signal was created.
    pub created_at: String,
}

// ── Decisions ────────────────────────────────────────────────────────────────

/// Action the agent chose to take in response to a signal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentAction {
    /// Trigger on-chain resolution/settlement.
    TriggerResolution,
    /// Fetch a proof receipt for verification.
    FetchProof,
    /// Simulate a position without committing.
    SimulatePosition,
    /// Notify the operator without taking action.
    Notify,
    /// Continue watching; take no action yet.
    Watch,
}

/// Lifecycle status of a decision's execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    /// Queued but not yet executed.
    Pending,
    /// Blocked by a policy check.
    Blocked,
    /// Executed successfully.
    Completed,
    /// Execution failed.
    Failed,
}

/// Outcome of a single named policy check applied to a decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyCheck {
    /// Name of the policy check.
    pub name: String,
    /// Whether the check passed.
    pub passed: bool,
    /// Detail explaining the result.
    pub detail: String,
}

/// A decision produced by the agent policy in response to a signal.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDecision {
    /// Unique identifier for this decision.
    pub id: String,
    /// Identifier of the signal that prompted the decision.
    pub signal_id: String,
    /// The action chosen.
    pub action: AgentAction,
    /// Confidence score in the range `[0.0, 1.0]`.
    pub confidence: f64,
    /// Policy checks applied to this decision.
    pub policy_checks: Vec<PolicyCheck>,
    /// Human-readable explanation of the decision.
    pub explanation: String,
    /// Current execution status.
    pub execution_status: ExecutionStatus,
    /// ISO-8601 timestamp of when the decision was created.
    pub created_at: String,
}
