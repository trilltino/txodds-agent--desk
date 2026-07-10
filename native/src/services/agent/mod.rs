//! Match Intelligence Agent runtime surface.
//!
//! This is deliberately one agent. Deterministic tools produce the signal and
//! proof/payment gates decide privileged outcomes; future LLM usage can narrate
//! or explain but cannot override these tool results.

pub mod authority;
pub mod context;
pub mod evaluation;
pub mod features;
pub mod policy;
pub mod pundit_agent;
pub mod runtime;
pub mod tools;
pub mod wager_agent;
