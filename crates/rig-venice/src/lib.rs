//! # rig-venice
//!
//! Thin factory layer that points `rig-core`'s OpenAI-compatible provider at
//! the Venice AI inference endpoint.  Every agent binary in this workspace
//! calls `rig_venice::client()` exactly once at startup — no API keys are
//! ever stored in structs that could leak into a prompt or a log.
//!
//! ## Environment variables
//! - `VENICE_API_KEY`     — required; Venice inference key
//! - `VENICE_MODEL`       — optional; defaults to `kimi-k2-7-code`
//! - `VENICE_PROSE_MODEL` — optional; defaults to `llama-3.3-70b` (see `prose_model_name`)
//! - `VENICE_BASE_URL`    — optional; defaults to Venice production endpoint

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

/// Default prose model — llama-3.3-70b was `sharp-movement-detector`'s own
/// `VENICE_MODEL` default before ROADMAP.md's "model default divergence is
/// resolved" pass unified everything onto `DEFAULT_MODEL` for speed and
/// consistency. `prose_model_name()` deliberately reintroduces that split —
/// not by accident, and not for the whole workspace: only the two
/// narrative-quality call sites that most benefit from a larger,
/// prose-oriented model (`fan-pundit-agent`, `sharp-movement-detector`'s own
/// signal narration) opt into it explicitly via this function, while
/// `model_name()`'s single default stays exactly as ROADMAP.md left it for
/// everything else.
pub const DEFAULT_PROSE_MODEL: &str = "llama-3.3-70b";

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

/// Return the configured prose model name (or `DEFAULT_PROSE_MODEL`) — for
/// call sites where narrative quality matters more than raw speed. See
/// `DEFAULT_PROSE_MODEL`'s doc comment for why this exists as a second,
/// explicit lever rather than changing `model_name()`'s default.
pub fn prose_model_name() -> String {
    std::env::var("VENICE_PROSE_MODEL").unwrap_or_else(|_| DEFAULT_PROSE_MODEL.to_owned())
}
