//! # rig-venice
//!
//! Thin factory layer that points `rig-core`'s OpenAI-compatible provider at
//! an inference endpoint. Every agent binary in this workspace calls
//! `rig_venice::client()` exactly once at startup — no API keys are ever
//! stored in structs that could leak into a prompt or a log.
//!
//! ## Providers
//!
//! Two providers are supported, both through the same OpenAI-compatible
//! `rig::providers::openai::Client` — Groq's API is "mostly compatible with
//! OpenAI's client libraries" by their own design, so this needed no new
//! abstraction, just a second base URL/key/model set:
//!
//! - **Venice** (default) — the original provider this crate was built for.
//! - **Groq** — added as a genuinely free fallback (`GROQ_API_KEY`, no paid
//!   tier required) for when Venice credits run out. Free-tier
//!   `llama-3.3-70b-versatile` is the same model family already chosen as
//!   `DEFAULT_PROSE_MODEL` below, so switching providers doesn't mean
//!   starting the model-quality conversation over.
//!
//! `LLM_PROVIDER=groq` selects Groq explicitly. Left unset, `active_provider()`
//! auto-detects: Venice if `VENICE_API_KEY` is set, else Groq if
//! `GROQ_API_KEY` is set, else Venice (preserving today's "VENICE_API_KEY not
//! set" failure as the default when nothing is configured at all).
//!
//! ## Environment variables
//! - `LLM_PROVIDER`     — optional; `"venice"` (default) or `"groq"`
//! - `VENICE_API_KEY`     — Venice inference key (required if provider is Venice)
//! - `VENICE_MODEL`       — optional; defaults to `kimi-k2-7-code`
//! - `VENICE_PROSE_MODEL` — optional; defaults to `llama-3.3-70b` (see `prose_model_name`)
//! - `VENICE_BASE_URL`    — optional; defaults to Venice production endpoint
//! - `GROQ_API_KEY`       — Groq inference key (required if provider is Groq)
//! - `GROQ_MODEL`         — optional; defaults to `llama-3.1-8b-instant` (fast, free-tier: 14,400 req/day)
//! - `GROQ_PROSE_MODEL`   — optional; defaults to `llama-3.3-70b-versatile` (free-tier: 1,000 req/day, 12k tok/min)
//! - `GROQ_BASE_URL`      — optional; defaults to Groq's OpenAI-compatible endpoint

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used)]

pub mod loop_runner;
pub mod tools;

use rig::providers::openai;
use tracing::info;

/// Venice OpenAI-compatible base URL.
pub const VENICE_BASE_URL: &str = "https://api.venice.ai/api/v1";

/// Groq's OpenAI-compatible base URL (confirmed against Groq's own docs:
/// "mostly compatible with OpenAI's client libraries").
pub const GROQ_BASE_URL: &str = "https://api.groq.com/openai/v1";

/// Default Venice model — kimi-k2 is Venice's fastest reasoning model.
pub const DEFAULT_MODEL: &str = "kimi-k2-7-code";

/// Default Groq model for the same speed-tuned role as `DEFAULT_MODEL` —
/// Llama 3.1 8B Instant, Groq's fastest general-purpose model and its most
/// generous free-tier limit (14,400 requests/day).
pub const DEFAULT_GROQ_MODEL: &str = "llama-3.1-8b-instant";

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

/// Default Groq prose model — same Llama 3.3 70B family as
/// `DEFAULT_PROSE_MODEL`, just Groq's exact model ID string
/// (`llama-3.3-70b-versatile`). Free tier: 1,000 requests/day, 12,000
/// tokens/minute — smaller than the 8B model's quota, but plenty for the two
/// narration call sites this is used for.
pub const DEFAULT_GROQ_PROSE_MODEL: &str = "llama-3.3-70b-versatile";

/// Which OpenAI-compatible provider `client()`/`model_name()`/
/// `prose_model_name()` should use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Venice,
    Groq,
}

/// Determine the active provider. Explicit `LLM_PROVIDER` wins; otherwise
/// auto-detect from whichever API key is actually configured, preferring
/// Venice for backward compatibility, falling through to Groq, and finally
/// defaulting to Venice so the "nothing configured" error still names
/// `VENICE_API_KEY` (today's behavior) rather than an arbitrary pick.
#[must_use]
pub fn active_provider() -> Provider {
    match std::env::var("LLM_PROVIDER").map(|value| value.to_ascii_lowercase()) {
        Ok(value) if value == "groq" => return Provider::Groq,
        Ok(value) if value == "venice" => return Provider::Venice,
        _ => {}
    }
    if std::env::var("VENICE_API_KEY").is_ok() {
        Provider::Venice
    } else if std::env::var("GROQ_API_KEY").is_ok() {
        Provider::Groq
    } else {
        Provider::Venice
    }
}

/// Errors that can occur when constructing the LLM client.
#[derive(Debug)]
pub enum VeniceError {
    /// The active provider's API key environment variable isn't set. Carries
    /// the variable name so the message names the right one regardless of
    /// which provider is active.
    MissingApiKey(&'static str),
}

impl std::fmt::Display for VeniceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VeniceError::MissingApiKey(var) => write!(f, "{var} environment variable not set"),
        }
    }
}

impl std::error::Error for VeniceError {}

/// Build a `rig` OpenAI-compatible client pointing at the active provider
/// (see `active_provider()`).
///
/// Reads the active provider's API key from the environment. Fails loudly
/// (returns `Err`) rather than silently falling back to an unauthenticated
/// client.
///
/// Checklist §21: "API keys loaded from a secrets manager, never hardcoded
/// or logged."
pub fn client() -> Result<openai::Client, VeniceError> {
    match active_provider() {
        Provider::Venice => {
            let api_key = std::env::var("VENICE_API_KEY")
                .map_err(|_| VeniceError::MissingApiKey("VENICE_API_KEY"))?;
            let base_url =
                std::env::var("VENICE_BASE_URL").unwrap_or_else(|_| VENICE_BASE_URL.to_owned());
            info!(provider = "venice", base_url = %base_url, "initialising Venice/rig client");
            Ok(openai::Client::from_url(&api_key, &base_url))
        }
        Provider::Groq => {
            let api_key = std::env::var("GROQ_API_KEY")
                .map_err(|_| VeniceError::MissingApiKey("GROQ_API_KEY"))?;
            let base_url =
                std::env::var("GROQ_BASE_URL").unwrap_or_else(|_| GROQ_BASE_URL.to_owned());
            info!(provider = "groq", base_url = %base_url, "initialising Groq/rig client");
            Ok(openai::Client::from_url(&api_key, &base_url))
        }
    }
}

/// Return the configured model name (or the active provider's default) for
/// speed-tuned call sites (tool-calling loops, not prose).
pub fn model_name() -> String {
    match active_provider() {
        Provider::Venice => std::env::var("VENICE_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_owned()),
        Provider::Groq => std::env::var("GROQ_MODEL").unwrap_or_else(|_| DEFAULT_GROQ_MODEL.to_owned()),
    }
}

/// Return the configured prose model name (or the active provider's prose
/// default) — for call sites where narrative quality matters more than raw
/// speed. See `DEFAULT_PROSE_MODEL`'s doc comment for why this exists as a
/// second, explicit lever rather than changing `model_name()`'s default.
pub fn prose_model_name() -> String {
    match active_provider() {
        Provider::Venice => {
            std::env::var("VENICE_PROSE_MODEL").unwrap_or_else(|_| DEFAULT_PROSE_MODEL.to_owned())
        }
        Provider::Groq => {
            std::env::var("GROQ_PROSE_MODEL").unwrap_or_else(|_| DEFAULT_GROQ_PROSE_MODEL.to_owned())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Every test that touches process env must run serialized — env vars are
    // global process state and `cargo test` runs tests on multiple threads by
    // default. Same technique as venice.rs's mock-Venice test in
    // sharp-movement-detector.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn clear_all_env() {
        for var in [
            "LLM_PROVIDER", "VENICE_API_KEY", "VENICE_MODEL", "VENICE_PROSE_MODEL", "VENICE_BASE_URL",
            "GROQ_API_KEY", "GROQ_MODEL", "GROQ_PROSE_MODEL", "GROQ_BASE_URL",
        ] {
            std::env::remove_var(var);
        }
    }

    #[test]
    fn defaults_to_venice_when_nothing_configured() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_all_env();
        assert_eq!(active_provider(), Provider::Venice);
        match client() {
            // `openai::Client` doesn't implement `Debug`, so `unwrap_err()`
            // isn't available here — match instead.
            Ok(_) => panic!("expected MissingApiKey, got Ok"),
            Err(err) => assert_eq!(err.to_string(), "VENICE_API_KEY environment variable not set"),
        }
    }

    #[test]
    fn auto_detects_groq_when_only_groq_key_present() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_all_env();
        std::env::set_var("GROQ_API_KEY", "test-groq-key");
        assert_eq!(active_provider(), Provider::Groq);
        assert!(client().is_ok());
        assert_eq!(model_name(), DEFAULT_GROQ_MODEL);
        assert_eq!(prose_model_name(), DEFAULT_GROQ_PROSE_MODEL);
        clear_all_env();
    }

    #[test]
    fn prefers_venice_when_both_keys_present_and_provider_unset() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_all_env();
        std::env::set_var("VENICE_API_KEY", "test-venice-key");
        std::env::set_var("GROQ_API_KEY", "test-groq-key");
        assert_eq!(active_provider(), Provider::Venice);
        clear_all_env();
    }

    #[test]
    fn explicit_llm_provider_groq_wins_even_with_venice_key_present() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_all_env();
        std::env::set_var("VENICE_API_KEY", "test-venice-key");
        std::env::set_var("LLM_PROVIDER", "groq");
        assert_eq!(active_provider(), Provider::Groq);
        // GROQ_API_KEY still isn't set — provider selection and key
        // presence are independent checks.
        match client() {
            Ok(_) => panic!("expected MissingApiKey, got Ok"),
            Err(err) => assert_eq!(err.to_string(), "GROQ_API_KEY environment variable not set"),
        }
        clear_all_env();
    }

    #[test]
    fn explicit_llm_provider_is_case_insensitive() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_all_env();
        std::env::set_var("LLM_PROVIDER", "GROQ");
        assert_eq!(active_provider(), Provider::Groq);
        clear_all_env();
    }

    #[test]
    fn groq_model_env_overrides_default() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_all_env();
        std::env::set_var("LLM_PROVIDER", "groq");
        std::env::set_var("GROQ_MODEL", "openai/gpt-oss-20b");
        assert_eq!(model_name(), "openai/gpt-oss-20b");
        clear_all_env();
    }

    #[test]
    fn venice_behavior_unchanged_when_explicitly_selected() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_all_env();
        std::env::set_var("LLM_PROVIDER", "venice");
        std::env::set_var("VENICE_API_KEY", "test-key");
        assert_eq!(active_provider(), Provider::Venice);
        assert_eq!(model_name(), DEFAULT_MODEL);
        assert_eq!(prose_model_name(), DEFAULT_PROSE_MODEL);
        clear_all_env();
    }

    /// Proves rig-core's actual multi-round tool-calling loop (not just a
    /// plain completion) works against Groq — every real agent in this
    /// workspace (fan-pundit-agent, sharp-movement-detector, wager_agent,
    /// pundit_agent) depends on exactly this: `client()` handing `.agent()` a
    /// `rig::providers::openai::Client`, then driving it through
    /// `loop_runner::run_tool_loop`. Uses `ComputeSharpMovement` (pure, no
    /// network) so the only real network call in this test is the Groq
    /// completion itself.
    ///
    /// Ignored by default (needs a real `GROQ_API_KEY` + `LLM_PROVIDER=groq`
    /// in the environment) — run explicitly with:
    /// `GROQ_API_KEY=... LLM_PROVIDER=groq cargo test -p rig-venice --lib -- --ignored --nocapture live_groq_tool_calling_loop`
    #[tokio::test]
    #[ignore]
    async fn live_groq_tool_calling_loop() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        if active_provider() != Provider::Groq {
            panic!("set LLM_PROVIDER=groq and GROQ_API_KEY in the environment to run this test");
        }

        let rig_client = match client() {
            Ok(c) => c,
            Err(e) => panic!("groq client should build: {e}"),
        };
        let model = model_name();
        let agent = rig_client
            .agent(&model)
            .preamble(
                "You analyse betting odds movement. Call compute_sharp_movement exactly once \
                 with the given numbers, then stop.",
            )
            .tool(tools::ComputeSharpMovement::default())
            .build();

        let outcome = match loop_runner::run_tool_loop(
            &agent,
            "Selection \"home\" in market \"1x2\" moved from previous odds 2.60 to current odds \
             2.35. Call compute_sharp_movement with these numbers."
                .to_string(),
            "compute_sharp_movement",
            3,
            || {},
        )
        .await
        {
            Ok(o) => o,
            Err(e) => panic!("tool loop should complete: {e}"),
        };

        if outcome.final_args.is_none() {
            panic!("agent should have called compute_sharp_movement at least once");
        }
        let Some(result) = outcome.tool_result("compute_sharp_movement") else {
            panic!("compute_sharp_movement's own output should be in the tool-call trace");
        };
        println!("groq tool-calling result: {result}");
        assert!(result.get("is_sharp_move").is_some(), "tool result should include is_sharp_move");
    }
}
