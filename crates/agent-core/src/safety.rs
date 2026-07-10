//! Agent safety gate — the runtime enforcement layer.
//!
//! This module answers Checklist §28 ("The Agent Safety Gate") in code.
//! Every item in §38 that can be enforced at runtime lives here.
//!
//! # What this module does
//!
//! 1. **Budget guard** — hard caps on tool-call count, lamport spend, and
//!    session duration.  Cannot be modified by the agent itself (the guard is
//!    constructed outside the agent loop and passed in read-only).
//!
//! 2. **Prompt injection delimiter** — the `wrap_untrusted` helper wraps any
//!    text from an external source (web page, tool result, another agent's
//!    message) in XML-style delimiters so the model sees a clear boundary
//!    between its own instructions and retrieved content.
//!    Checklist §28: "content from untrusted sources clearly delimited from
//!    trusted instructions in the prompt".
//!
//! 3. **Consecutive-step cap** — a per-session counter that the agent loop
//!    increments on every step.  Once the cap is hit the session must stop
//!    or escalate to a human.
//!    Checklist §14: "a hard cap on consecutive tool calls to bound runaway
//!    loops".
//!
//! There used to be a kill switch here (a shared atomic flag any thread,
//! including an OS signal handler, could trip to abort the session on the
//! next `safety_check` call). Per explicit product decision it has been
//! removed system-wide — see `crates/rig-venice/ROADMAP.md`, "Removing the
//! kill switch". `BudgetGuard` and `StepCounter` are unrelated rate/step
//! limits and are unaffected by that decision.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::error::{AgentError, BudgetResource};

// ── Budget guard ──────────────────────────────────────────────────────────────

/// Hard resource limits for a single agent session.
///
/// Checklist §28: "rate limit / spend limit per session and per time window
/// that cannot be raised by the agent itself".
///
/// The struct is intentionally non-`Clone` and non-`Copy` so that it cannot
/// accidentally be captured into the agent's own context.  The agent receives
/// a shared reference only.
#[derive(Debug)]
pub struct BudgetGuard {
    /// Maximum number of tool calls in this session (default: 200).
    max_tool_calls: u64,
    /// Maximum lamports the agent may cause to be spent (default: `1_000_000`).
    max_spend_lamports: u64,
    /// Maximum wall-clock session duration in seconds (default: 3600).
    max_duration_secs: u64,

    // Atomic counters — incremented by the agent loop, checked before each call.
    tool_calls: Arc<AtomicU64>,
    spend_lamports: Arc<AtomicU64>,
    /// Unix timestamp (seconds) when the session started.
    started_at: u64,
}

impl BudgetGuard {
    /// Build a guard with explicit limits.
    /// Call this from the session setup code, not from within the agent loop.
    #[must_use]
    pub fn new(
        max_tool_calls: u64,
        max_spend_lamports: u64,
        max_duration_secs: u64,
    ) -> Self {
        Self {
            max_tool_calls,
            max_spend_lamports,
            max_duration_secs,
            tool_calls: Arc::new(AtomicU64::new(0)),
            spend_lamports: Arc::new(AtomicU64::new(0)),
            started_at: unix_now(),
        }
    }

    /// Conservative defaults suitable for a hackathon/devnet agent.
    #[must_use]
    pub fn default_devnet() -> Self {
        Self::new(200, 1_000_000, 3_600)
    }

    /// Record a completed tool call (must be called after every tool execution).
    pub fn record_tool_call(&self) {
        self.tool_calls.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a lamport expenditure (on-chain TX fee + any SOL transferred).
    pub fn record_spend(&self, lamports: u64) {
        self.spend_lamports.fetch_add(lamports, Ordering::Relaxed);
    }

    /// Check all budget limits.  Returns `Err` if any limit is exceeded.
    pub fn check(&self) -> Result<(), AgentError> {
        let calls = self.tool_calls.load(Ordering::Relaxed);
        if calls >= self.max_tool_calls {
            return Err(AgentError::BudgetExceeded {
                resource: BudgetResource::ToolCallCount,
                limit: self.max_tool_calls,
                current: calls,
            });
        }

        let spend = self.spend_lamports.load(Ordering::Relaxed);
        if spend >= self.max_spend_lamports {
            return Err(AgentError::BudgetExceeded {
                resource: BudgetResource::SpendLamports,
                limit: self.max_spend_lamports,
                current: spend,
            });
        }

        let elapsed = unix_now().saturating_sub(self.started_at);
        if elapsed >= self.max_duration_secs {
            return Err(AgentError::BudgetExceeded {
                resource: BudgetResource::SessionDurationSeconds,
                limit: self.max_duration_secs,
                current: elapsed,
            });
        }

        Ok(())
    }

    /// Return the number of tool calls recorded so far.
    #[must_use]
    pub fn current_tool_calls(&self) -> u64 {
        self.tool_calls.load(Ordering::Relaxed)
    }

    /// Return the total lamports spent so far.
    #[must_use]
    pub fn current_spend_lamports(&self) -> u64 {
        self.spend_lamports.load(Ordering::Relaxed)
    }
}

// ── Combined safety check ─────────────────────────────────────────────────────

/// Run the budget guard.
///
/// Insert this at the top of every agent loop iteration.
/// Checklist §28, §38: "fails closed on any error it wasn't designed to handle".
pub fn safety_check(budget: &BudgetGuard) -> Result<(), AgentError> {
    budget.check()
}

// ── Prompt injection delimiter ────────────────────────────────────────────────

/// Wrap text from an untrusted source in XML-style delimiters.
///
/// These delimiters tell the model that the enclosed content is external
/// data, **not** a new instruction.  They are not a complete defence against
/// prompt injection, but they are the minimum structural hint a well-designed
/// system prompt can leverage.
///
/// Checklist §28: "content from untrusted sources clearly delimited".
/// Checklist §15: "summarized memory re-validated before being trusted".
#[must_use]
pub fn wrap_untrusted(source_label: &str, content: &str) -> String {
    // Hard limit to prevent a runaway tool response from filling context.
    // Checklist §20: "maximum size limit on tool responses being parsed".
    const MAX_CONTENT_BYTES: usize = 32_768; // 32 KiB

    let truncated = if content.len() > MAX_CONTENT_BYTES {
        &content[..MAX_CONTENT_BYTES]
    } else {
        content
    };

    format!(
        "<untrusted_source label=\"{source_label}\">\n{truncated}\n</untrusted_source>"
    )
}

// ── Consecutive-step cap ──────────────────────────────────────────────────────

/// Per-session step counter with a hard cap.
///
/// Checklist §14: "hard cap on consecutive tool calls to bound runaway loops".
#[derive(Debug)]
pub struct StepCounter {
    max_steps: u64,
    steps: u64,
}

impl StepCounter {
    /// Create a new step counter with the given maximum.
    #[must_use]
    pub fn new(max_steps: u64) -> Self {
        Self { max_steps, steps: 0 }
    }

    /// Increment the counter and fail closed if the cap is hit.
    pub fn tick(&mut self) -> Result<(), AgentError> {
        self.steps += 1;
        if self.steps > self.max_steps {
            Err(AgentError::BudgetExceeded {
                resource: BudgetResource::ToolCallCount,
                limit: self.max_steps,
                current: self.steps,
            })
        } else {
            Ok(())
        }
    }

    /// Return the number of steps taken so far.
    #[must_use]
    pub fn current(&self) -> u64 {
        self.steps
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn unix_now() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safety_check_passes_when_budget_ok() {
        let budget = BudgetGuard::default_devnet();
        assert!(safety_check(&budget).is_ok());
    }

    #[test]
    fn budget_tool_call_limit() {
        let guard = BudgetGuard::new(2, u64::MAX, u64::MAX);
        guard.record_tool_call();
        guard.record_tool_call();
        assert!(matches!(
            guard.check(),
            Err(AgentError::BudgetExceeded {
                resource: BudgetResource::ToolCallCount,
                ..
            })
        ));
    }

    #[test]
    fn wrap_untrusted_adds_delimiters() {
        let wrapped = wrap_untrusted("tool_result", "hello world");
        assert!(wrapped.contains("<untrusted_source label=\"tool_result\">"));
        assert!(wrapped.contains("hello world"));
        assert!(wrapped.contains("</untrusted_source>"));
    }

    #[test]
    fn wrap_untrusted_truncates_large_content() {
        let big = "x".repeat(100_000);
        let wrapped = wrap_untrusted("big_doc", &big);
        // Should be less than 100KB + a small overhead
        assert!(wrapped.len() < 40_000);
    }

    #[test]
    fn step_counter_fails_at_cap() {
        let mut counter = StepCounter::new(3);
        assert!(counter.tick().is_ok());
        assert!(counter.tick().is_ok());
        assert!(counter.tick().is_ok());
        assert!(counter.tick().is_err());
    }
}
