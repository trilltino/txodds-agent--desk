//! `agent-core` — pure deterministic agent logic for the `TxLINE` arena.
//!
//! No Tauri, no async runtime, no reqwest.  Run tests with:
//!   cargo test -p agent-core
//!
//! # Crate layout
//!
//! | Module        | Purpose                                                 |
//! |---------------|---------------------------------------------------------|
//! | `capability`  | Compile-time capability tokens (§8)                     |
//! | `tools`       | `Tool` trait + `IdempotencyKey` (§7, §14)               |
//! | `safety`      | Kill switch, budget guard, injection delimiters (§28)   |
//! | `error`       | Agent error taxonomy with retry policy (§9)             |
//! | `domain`      | Signal, decision, and proof-gate types                  |
//! | `context`     | Session context and working memory                      |
//! | `features`    | Feature extraction from TxLINE odds snapshots           |
//! | `policy`      | Policy checks (confidence gates, signal filters)        |
//! | `evaluation`  | Outcome evaluation and backtesting helpers              |
//! | `arena`       | Agent-vs-agent arena position and scoring types         |

#![forbid(unsafe_code)]
// Checklist §9: unwrap/expect banned on paths touching external input.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::todo)]
#![warn(missing_docs, clippy::pedantic)]
// Allow a few pedantic lints that are overly strict for this codebase.
#![allow(clippy::module_name_repetitions, clippy::missing_errors_doc)]

pub mod capability;
pub mod domain;
pub mod context;
pub mod evaluation;
pub mod features;
pub mod policy;
pub mod arena;

// New modules added by this iteration:
pub mod error;
pub mod safety;
pub mod tools;

// Ported from coral-agents/ Python (now deleted):
pub mod fundamentals;
pub mod proof_guard;

// ── Top-level re-exports ──────────────────────────────────────────────────────

pub use capability::{CapabilityGrant, Capability, FadeCap, FollowCap, SettleCap};
pub use domain::ProofGateDecision;
pub use error::{AgentError, is_retryable};
pub use safety::{BudgetGuard, StepCounter, safety_check, wrap_untrusted};
pub use tools::{IdempotencyKey, Tool, ToolCallOutcome, ToolCallRecord, ToolTrailEntry};
