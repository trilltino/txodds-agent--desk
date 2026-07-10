//! Agent error taxonomy.
//!
//! Checklist §9: "Is there a distinct error type for 'the model produced
//! something we can't safely act on' vs 'a transient network error' vs
//! 'a genuine tool failure', so retries don't blindly retry unsafe requests?"
//!
//! # Retry policy
//!
//! | Variant            | Safe to auto-retry? |
//! |--------------------|---------------------|
//! | `Transient`        | Yes — exponential backoff, max 3 attempts |
//! | `ModelOutput`      | No  — re-executing the same bad output is dangerous |
//! | `ToolFail`         | No  — tool already attempted; idempotency key must change |
//! | `BudgetExceeded`   | No  — escalate to human, do not retry |
//! | `Timeout`          | No  — side-effect status unknown; report "unknown" |

use std::fmt;

/// All errors that can occur inside the agent loop.
#[derive(Debug)]
pub enum AgentError {
    /// A transient infrastructure error (network blip, HTTP 429, DNS failure).
    /// Safe to retry with exponential backoff.
    Transient {
        /// The underlying error.
        source: Box<dyn std::error::Error + Send + Sync>,
        /// Which retry attempt this represents (1-based).
        attempt: u32,
    },

    /// The model produced output that cannot be safely parsed or acted upon.
    /// **Not safe to retry** — the same prompt will produce the same bad output.
    /// Fail closed and log the raw output for forensic review.
    ModelOutput {
        /// The raw model output that could not be parsed.
        raw: String,
        /// Why the output was rejected.
        reason: String,
    },

    /// A tool invocation failed after the tool boundary was crossed.
    /// **Not safe to retry** — the tool may have already performed a side effect.
    /// The caller must inspect `side_effect_status` before deciding.
    ToolFail {
        /// Name of the tool that failed.
        tool_name: &'static str,
        /// Idempotency key for the failed call.
        idempotency_key: String,
        /// Whether the side effect occurred before the failure.
        side_effect_status: SideEffectStatus,
        /// The underlying error.
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// The session hit its hard budget cap (tool calls, spend, or duration).
    /// Checklist §28, §38: budget cannot be raised by the agent itself.
    BudgetExceeded {
        /// Which budget resource was exhausted.
        resource: BudgetResource,
        /// The configured limit.
        limit: u64,
        /// The current value that exceeded the limit.
        current: u64,
    },

    /// An outbound call exceeded its deadline.
    /// The side-effect status is *unknown* — do not assume success or failure.
    Timeout {
        /// Name of the operation that timed out.
        operation: &'static str,
        /// The deadline in milliseconds that was exceeded.
        deadline_ms: u64,
    },

    /// A named tool call returned an error before any side effect was confirmed.
    /// Convenience variant for agent binaries that use simple HTTP tools.
    ToolCallFailed {
        /// Name of the tool that failed.
        tool: String,
        /// Why the call failed.
        reason: String,
    },

    /// Parsing/deserialization of an external payload failed.
    /// The payload (model output or API response) should be logged for forensics.
    ParseError(String),
}

/// Whether a tool call's side effect is known to have happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SideEffectStatus {
    /// The side effect definitely did not happen (pre-execution failure).
    NotExecuted,
    /// The side effect status is unknown (timeout / lost connection).
    Unknown,
    /// The side effect happened but the tool returned an error post-execution.
    ExecutedWithError,
}

/// Which budget resource was exhausted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BudgetResource {
    /// The maximum number of tool calls was reached.
    ToolCallCount,
    /// The maximum lamport spend was reached.
    SpendLamports,
    /// The maximum session wall-clock duration was reached.
    SessionDurationSeconds,
}

impl fmt::Display for AgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transient { source, attempt } => {
                write!(f, "transient error (attempt {attempt}): {source}")
            }
            Self::ModelOutput { reason, .. } => {
                write!(f, "unsafe model output: {reason}")
            }
            Self::ToolFail {
                tool_name,
                idempotency_key,
                side_effect_status,
                source,
            } => {
                write!(
                    f,
                    "tool '{tool_name}' failed (key={idempotency_key}, \
                     side_effect={side_effect_status:?}): {source}"
                )
            }
            Self::BudgetExceeded {
                resource,
                limit,
                current,
            } => {
                write!(
                    f,
                    "budget exceeded for {resource:?}: limit={limit}, current={current}"
                )
            }
            Self::Timeout {
                operation,
                deadline_ms,
            } => {
                write!(
                    f,
                    "operation '{operation}' timed out after {deadline_ms}ms \
                     — side-effect status unknown"
                )
            }
            Self::ToolCallFailed { tool, reason } => {
                write!(f, "tool call '{tool}' failed: {reason}")
            }
            Self::ParseError(msg) => {
                write!(f, "parse error: {msg}")
            }
        }
    }
}

impl std::error::Error for AgentError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Transient { source, .. } | Self::ToolFail { source, .. } => Some(source.as_ref()),
            _ => None,
        }
    }
}

/// Returns `true` if this error is safe to retry.
/// Checklist §9, §14.
#[must_use]
pub fn is_retryable(err: &AgentError) -> bool {
    matches!(err, AgentError::Transient { .. })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_is_retryable() {
        let err = AgentError::Transient {
            source: "connection reset".into(),
            attempt: 1,
        };
        assert!(is_retryable(&err));
    }

    #[test]
    fn model_output_is_not_retryable() {
        let err = AgentError::ModelOutput {
            raw: "bad json".into(),
            reason: "parse failed".into(),
        };
        assert!(!is_retryable(&err));
    }

    #[test]
    fn timeout_is_not_retryable() {
        let err = AgentError::Timeout {
            operation: "record_position",
            deadline_ms: 15_000,
        };
        assert!(!is_retryable(&err));
    }
}
