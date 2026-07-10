//! TxLINE fixture/odds HTTP client and response types.

use std::time::Duration;

use agent_core::error::AgentError;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct FixtureListResp {
    pub(crate) data: Vec<FixtureSummary>,
}

#[derive(Debug, Deserialize, Clone)]
pub(crate) struct FixtureSummary {
    pub(crate) id: u64,
    pub(crate) name: String,
    pub(crate) status: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub(crate) struct OddsSnapshot {
    fixture_id: u64,
    pub(crate) markets: Vec<Market>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Market {
    pub(crate) key: String,
    pub(crate) selections: Vec<Selection>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub(crate) struct Selection {
    pub(crate) name: String,
    pub(crate) odds: f64,
    previous_odds: Option<f64>,
}

pub(crate) async fn fetch_live_fixtures(
    client: &reqwest::Client,
    base: &str,
) -> Result<Vec<FixtureSummary>, AgentError> {
    let url = format!("{base}/worldcup/fixtures?status=live");
    let resp = client.get(&url).send().await.map_err(|e| AgentError::ToolCallFailed {
        tool: "fetch_live_fixtures".into(),
        reason: e.to_string(),
    })?;
    if !resp.status().is_success() {
        return Err(AgentError::ToolCallFailed {
            tool: "fetch_live_fixtures".into(),
            reason: format!("HTTP {}", resp.status()),
        });
    }
    let list: FixtureListResp = resp
        .json()
        .await
        .map_err(|e| AgentError::ParseError(e.to_string()))?;
    Ok(list.data.into_iter().filter(|f| f.status == "live").collect())
}

pub(crate) async fn fetch_odds(
    client: &reqwest::Client,
    base: &str,
    fixture_id: u64,
) -> Result<OddsSnapshot, AgentError> {
    let url = format!("{base}/worldcup/fixtures/{fixture_id}/odds");
    let resp = client.get(&url).send().await.map_err(|e| AgentError::ToolCallFailed {
        tool: "fetch_odds".into(),
        reason: e.to_string(),
    })?;
    if !resp.status().is_success() {
        return Err(AgentError::ToolCallFailed {
            tool: "fetch_odds".into(),
            reason: format!("HTTP {}", resp.status()),
        });
    }
    resp.json().await.map_err(|e| AgentError::ParseError(e.to_string()))
}

pub(crate) fn build_txline_client(api_key: &str) -> Result<reqwest::Client, String> {
    let mut headers = reqwest::header::HeaderMap::new();
    let val = reqwest::header::HeaderValue::from_str(&format!("Bearer {api_key}"))
        .map_err(|e| format!("invalid TXLINE_API_KEY header value: {e}"))?;
    headers.insert(reqwest::header::AUTHORIZATION, val);
    reqwest::Client::builder()
        .default_headers(headers)
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("failed to build TxLINE HTTP client: {e}"))
}
