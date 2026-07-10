//! # rig-venice
//!
//! Thin factory layer that points `rig-core`'s OpenAI-compatible provider at
//! the Venice AI inference endpoint.  Every agent binary in this workspace
//! calls `rig_venice::client()` exactly once at startup — no API keys are
//! ever stored in structs that could leak into a prompt or a log.
//!
//! ## Environment variables
//! - `VENICE_API_KEY`   — required; Venice inference key
//! - `VENICE_MODEL`     — optional; defaults to `kimi-k2-7-code`
//! - `VENICE_BASE_URL`  — optional; defaults to Venice production endpoint

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used)]

pub mod loop_runner;
pub mod tools;

use rig::providers::openai;
use tracing::info;

/// Venice OpenAI-compatible base URL.
pub const VENICE_BASE_URL: &str = "https://api.venice.ai/api/v1";

/// Default model — kimi-k2 is Venice's fastest reasoning model.
pub const DEFAULT_MODEL: &str = "kimi-k2-7-code";

/// Errors that can occur when constructing the Venice client.
#[derive(Debug)]
pub enum VeniceError {
    MissingApiKey,
}

impl std::fmt::Display for VeniceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VeniceError::MissingApiKey => write!(f, "VENICE_API_KEY environment variable not set"),
        }
    }
}

impl std::error::Error for VeniceError {}

/// Build a `rig` OpenAI-compatible client pointing at Venice AI.
///
/// Reads `VENICE_API_KEY` from the environment.  Fails loudly (returns `Err`)
/// rather than silently falling back to an unauthenticated client.
///
/// Checklist §21: "API keys loaded from a secrets manager, never hardcoded
/// or logged."
pub fn client() -> Result<openai::Client, VeniceError> {
    let api_key = std::env::var("VENICE_API_KEY").map_err(|_| VeniceError::MissingApiKey)?;

    let base_url = std::env::var("VENICE_BASE_URL")
        .unwrap_or_else(|_| VENICE_BASE_URL.to_owned());

    info!(base_url = %base_url, "initialising Venice/rig client");

    Ok(openai::Client::from_url(&api_key, &base_url))
}

/// Return the configured model name (or the default).
pub fn model_name() -> String {
    std::env::var("VENICE_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_owned())
}
