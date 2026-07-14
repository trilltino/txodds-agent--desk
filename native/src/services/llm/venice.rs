//! OpenAI-compatible chat completions adapter for round narration.
//!
//! Supports two providers behind `config.llm_provider` — Venice (original)
//! and Groq (added as a genuinely free fallback for when Venice credits run
//! out; Groq's API is "mostly compatible with OpenAI's client libraries" by
//! their own design, so this is a second base URL/key/model set, not a new
//! request shape). This is a separate, in-process client from
//! `crates/rig-venice` — used only for this round's own narration text — so
//! both need the Groq switch for a Venice outage to stop degrading the app
//! end-to-end; fixing one without the other still leaves the other silently
//! falling back to a deterministic, LLM-free explanation.

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::config::AppConfig;
use crate::error::AppError;

use super::schemas::{LlmRequest, LlmResponse};

const VENICE_COMPLETIONS_URL: &str = "https://api.venice.ai/api/v1/chat/completions";
const GROQ_COMPLETIONS_URL: &str = "https://api.groq.com/openai/v1/chat/completions";
/// Groq's fast, generous-free-tier default (14,400 requests/day) — used
/// whenever `config.llm_model` still holds a Venice-specific model name
/// (the "default"/"kimi-k2-7-code" values `.env`/config.rs ship) rather than
/// something the caller explicitly customized for Groq.
const DEFAULT_GROQ_MODEL: &str = "llama-3.1-8b-instant";

#[derive(Clone)]
pub struct VeniceClient {
    http: Client,
}

impl VeniceClient {
    pub fn new(http: Client) -> Self {
        Self { http }
    }

    pub async fn complete(
        &self,
        config: &AppConfig,
        request: LlmRequest,
    ) -> Result<LlmResponse, AppError> {
        match config.llm_provider.to_ascii_lowercase().as_str() {
            "venice" => {
                let model = request.model.clone();
                self.complete_with(
                    config.venice_api_key.as_deref(),
                    "missing_venice_api_key",
                    "Venice is not configured; deterministic explanation used.",
                    VENICE_COMPLETIONS_URL,
                    "venice",
                    model,
                    request,
                )
                .await
            }
            "groq" => {
                // A Venice-specific model name would 400 against Groq —
                // substitute Groq's own default unless the caller already
                // pointed this at a real Groq model id.
                let model = if request.model == "default" || request.model.eq_ignore_ascii_case("kimi-k2-7-code") {
                    DEFAULT_GROQ_MODEL.to_string()
                } else {
                    request.model.clone()
                };
                self.complete_with(
                    config.groq_api_key.as_deref(),
                    "missing_groq_api_key",
                    "Groq is not configured; deterministic explanation used.",
                    GROQ_COMPLETIONS_URL,
                    "groq",
                    model,
                    request,
                )
                .await
            }
            other => Ok(LlmResponse::fallback(
                "LLM provider is not Venice or Groq; deterministic explanation used.",
                format!("unsupported_provider:{other}"),
            )),
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn complete_with(
        &self,
        api_key: Option<&str>,
        missing_key_reason: &str,
        missing_key_text: &str,
        url: &str,
        provider_name: &str,
        model: String,
        request: LlmRequest,
    ) -> Result<LlmResponse, AppError> {
        let Some(api_key) = api_key else {
            return Ok(LlmResponse::fallback(missing_key_text, missing_key_reason));
        };

        let payload = ChatCompletionRequest {
            model: model.clone(),
            temperature: request.temperature,
            max_tokens: max_tokens(&model, request.max_tokens),
            messages: vec![
                ChatMessage {
                    role: "system",
                    content: request.system,
                },
                ChatMessage {
                    role: "user",
                    content: request.user,
                },
            ],
        };

        let response = self
            .http
            .post(url)
            .bearer_auth(api_key)
            .json(&payload)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(AppError::Task(format!("{provider_name} HTTP {}", response.status())));
        }

        let body = response.json::<ChatCompletionResponse>().await?;
        let text = body
            .choices
            .first()
            .map(|choice| choice.message.content.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| format!("{provider_name} returned an empty explanation."));

        Ok(LlmResponse {
            provider: provider_name.to_string(),
            model,
            text,
            used: true,
            reason: None,
        })
    }
}

fn max_tokens(model: &str, requested: u32) -> u32 {
    if model.to_ascii_lowercase().contains("kimi") {
        requested.max(1024)
    } else {
        requested.max(256)
    }
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    max_tokens: u32,
}

#[derive(Debug, Serialize)]
struct ChatMessage {
    role: &'static str,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatChoiceMessage,
}

#[derive(Debug, Deserialize)]
struct ChatChoiceMessage {
    content: String,
}

#[cfg(test)]
mod tests {
    use super::max_tokens;

    #[test]
    fn kimi_models_get_a_larger_floor() {
        assert_eq!(max_tokens("kimi-k2-7-code", 300), 1024);
        assert_eq!(max_tokens("other", 300), 300);
    }
}
