//! Venice OpenAI-compatible chat completions adapter.

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::config::AppConfig;
use crate::error::AppError;

use super::schemas::{LlmRequest, LlmResponse};

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
        if !config.llm_provider.eq_ignore_ascii_case("venice") {
            return Ok(LlmResponse::fallback(
                "LLM provider is not Venice; deterministic explanation used.",
                format!("unsupported_provider:{}", config.llm_provider),
            ));
        }

        let Some(api_key) = config.venice_api_key.as_deref() else {
            return Ok(LlmResponse::fallback(
                "Venice is not configured; deterministic explanation used.",
                "missing_venice_api_key",
            ));
        };

        let payload = ChatCompletionRequest {
            model: request.model.clone(),
            temperature: request.temperature,
            max_tokens: max_tokens(&request.model, request.max_tokens),
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
            .post("https://api.venice.ai/api/v1/chat/completions")
            .bearer_auth(api_key)
            .json(&payload)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(AppError::Task(format!("Venice HTTP {}", response.status())));
        }

        let body = response.json::<ChatCompletionResponse>().await?;
        let text = body
            .choices
            .first()
            .map(|choice| choice.message.content.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "Venice returned an empty explanation.".to_string());

        Ok(LlmResponse {
            provider: "venice".to_string(),
            model: request.model,
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
