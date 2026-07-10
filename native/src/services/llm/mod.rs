//! Optional LLM explanation client.
//!
//! The Match Intelligence Agent uses the LLM only to explain deterministic
//! feature/policy outputs. Proof gates, payments, and settlement readiness stay
//! code-owned.

pub mod schemas;
pub mod venice;

pub use schemas::{LlmRequest, LlmResponse};
pub use venice::VeniceClient;
