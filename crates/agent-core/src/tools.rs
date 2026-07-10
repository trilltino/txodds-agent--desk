//! Core `Tool` trait and idempotency key types.
//!
//! Checklist §7: "Is there a `Tool` trait with a typed input/output schema,
//! not a stringly-typed `fn call(json: &str) -> String`?"
//!
//! Checklist §14: "Does every tool call that has a side effect carry an
//! idempotency key, so a retried call after a timeout cannot double-execute?"
//!
//! # Design
//!
//! Every tool is generic over a `Capability` token (`C`).  The compiler
//! ensures that only agents holding the correct capability token can
//! instantiate a tool that requires it.  This is a compile-time guarantee,
//! not a runtime permission check.
//!
//! ```rust,ignore
//! // This compiles — match-intelligence-agent has FollowCap:
//! let tool = RecordPositionTool::new(CapabilityGrant::new(FollowCap));
//!
//! // This does NOT compile — contrarian-agent only has FadeCap:
//! let tool = RecordPositionTool::<FollowCap>::new(grant_with_fade_cap);
//! //                                             ^^^^^^^^^^^^^^^^^^^
//! //                             type mismatch: FollowCap vs FadeCap
//! ```

use std::fmt;
use crate::capability::Capability;
use crate::error::AgentError;

// ── Idempotency key ───────────────────────────────────────────────────────────

/// An opaque, unique key for a single tool invocation.
///
/// If a call times out, the coordinator retries with the **same key**.
/// The on-chain program (and any external API) must be idempotent on this key —
/// a duplicate call with the same key is a no-op, not a double-execution.
///
/// Construction is intentionally verbose so callers cannot accidentally pass an
/// empty or truncated string.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IdempotencyKey(String);

impl IdempotencyKey {
    /// Build a key from deterministic inputs.
    ///
    /// # Arguments
    /// * `agent_id`   — stable identifier for the agent instance
    /// * `fixture_id` — `TxLINE` fixture the call relates to
    /// * `sequence`   — monotonically increasing counter per agent session
    #[must_use]
    pub fn new(agent_id: &str, fixture_id: u64, sequence: u64) -> Self {
        // SHA-256-like deterministic string; no crypto crate needed for a key.
        // In production this is typically a UUID-v5 over the three inputs.
        Self(format!("{agent_id}:{fixture_id}:{sequence}"))
    }

    /// Build a key from a single deterministic content string.
    /// Use when you don't have a structured (`agent_id`, `fixture_id`, sequence)
    /// triple available — e.g. inside a simple HTTP agent binary.
    #[must_use]
    pub fn new_for(content: &str) -> Self {
        Self(content.to_owned())
    }

    /// Expose the inner string (e.g. to attach to an HTTP header or on-chain arg).
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for IdempotencyKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

// ── Tool trait ────────────────────────────────────────────────────────────────

/// A stateless, typed tool that an agent can call.
///
/// `Input` and `Output` are fully typed — the trait does **not** accept raw
/// JSON strings.  Serialization/validation happens at the MCP boundary
/// (in the adapter crate), never inside the business logic.
///
/// The `Cap` associated type is the capability token required to call this
/// tool.  If an agent does not hold a `CapabilityGrant<Cap>`, it cannot
/// construct or call the tool at compile time.
pub trait Tool: Send + Sync + 'static {
    /// Typed, validated input — must derive `serde::Deserialize` and `JsonSchema`.
    type Input: Send + 'static;

    /// Typed output — must derive `serde::Serialize`.
    type Output: Send + 'static;

    /// The capability token required to execute this tool.
    /// Use `()` for read-only tools that require no special grant.
    type Cap: Capability;

    /// Human-readable name — used in audit logs and `CoralOS` tool registry.
    fn name(&self) -> &'static str;

    /// One-line description shown in the `CoralOS` MCP tool schema.
    fn description(&self) -> &'static str;

    /// Derive a deterministic idempotency key from the input.
    /// Called *before* execution so it can be attached to the audit log
    /// regardless of whether the call succeeds or times out.
    fn idempotency_key(&self, input: &Self::Input, sequence: u64) -> IdempotencyKey;

    /// Execute the tool.
    ///
    /// # Safety contract
    /// - Implementations MUST NOT panic on any reachable code path.
    /// - Side effects MUST be idempotent on `idempotency_key`.
    /// - On timeout the caller holds the `IdempotencyKey` and can query status.
    fn execute(
        &self,
        input: Self::Input,
        cap: &crate::capability::CapabilityGrant<Self::Cap>,
    ) -> impl std::future::Future<Output = Result<Self::Output, AgentError>> + Send;
}

// ── Read-only tool capability ─────────────────────────────────────────────────

/// Capability token for read-only tools (odds snapshots, fixture lookups).
/// Every agent may hold this — no special grant needed.
#[derive(Debug, Clone, Copy)]
pub struct ReadCap;

impl crate::capability::private_sealed::Sealed for ReadCap {}
impl Capability for ReadCap {}

// ── Tool call record for audit log ───────────────────────────────────────────

/// An immutable record of a single tool invocation, written to the audit log
/// *before* execution begins and updated once the result is known.
///
/// Checklist §24: "tamper-evident audit log of every tool call an agent made,
/// its arguments, its result, and whether it was allowed or blocked."
#[derive(Debug, Clone)]
pub struct ToolCallRecord {
    /// Unique per-session trace ID (propagated from `CoralOS` session).
    pub trace_id: String,
    /// Agent that made the call.
    pub agent_id: String,
    /// Tool name.
    pub tool_name: &'static str,
    /// Idempotency key for this call.
    pub idempotency_key: IdempotencyKey,
    /// ISO-8601 timestamp when the call was *proposed*.
    pub proposed_at: String,
    /// Whether the capability check passed before execution.
    pub capability_granted: bool,
    /// Outcome after execution.
    pub outcome: ToolCallOutcome,
}

/// The outcome of a tool call, recorded in the audit log.
#[derive(Debug, Clone)]
pub enum ToolCallOutcome {
    /// Not yet known — record written pre-execution.
    Pending,
    /// Tool executed successfully.
    Success,
    /// Tool call blocked by budget or kill switch before execution.
    Blocked {
        /// Why the call was blocked.
        reason: String,
    },
    /// Tool execution failed; side effect status is attached.
    Failed {
        /// Brief description of the failure.
        error_summary: String,
    },
    /// Deadline exceeded; side-effect status is unknown.
    TimedOut,
}
