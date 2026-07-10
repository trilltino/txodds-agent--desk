//! LLM request/response contracts kept small enough for audit logging.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmRequest {
    pub system: String,
    pub user: String,
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmResponse {
    pub provider: String,
    pub model: String,
    pub text: String,
    pub used: bool,
    pub reason: Option<String>,
}

impl LlmResponse {
    pub fn fallback(text: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            provider: "none".to_string(),
            model: "deterministic".to_string(),
            text: text.into(),
            used: false,
            reason: Some(reason.into()),
        }
    }
}
