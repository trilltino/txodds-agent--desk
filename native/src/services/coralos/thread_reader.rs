//! Reads back real messages a genuinely independent CoralOS process posted.
//!
//! `console.rs` can only POST (the puppet API). This is the one place
//! `native/` actually reads the bus: `GET
//! /api/v1/local/session/{namespace}/{session}/extended`, returning full
//! session state and per-thread transcripts. Response shape verified
//! against a live `coral-server` (see `pay`'s
//! `examples/txodds/feed/src/coralState.ts`, which parses the same
//! endpoint):
//!
//! ```json
//! { "agents": [ { "name": "...", "status": { "type": "..." } } ],
//!   "threads": [ { "id": "...", "participants": [...], "messages": [
//!     { "threadId": "...", "senderName": "...", "text": "...",
//!       "mentionNames": [...], "timestamp": "..." } ] } ] }
//! ```
//!
//! Polling this is what makes the orchestrator's outcome genuinely depend
//! on another process's output: if `proof-guard-agent` never replies (killed,
//! never started, wrong image), this times out and the caller must fail
//! closed — there is no fallback value to compute locally, because the
//! whole point is that this Rust process no longer computes it.

use std::time::Duration;

use reqwest::Client;
use serde_json::Value;

use crate::config::AppConfig;

use super::console::LiveSession;

/// One message read back from a live thread.
#[derive(Debug, Clone)]
pub struct ThreadMessage {
    pub text: String,
}

/// Poll `live`'s session for a message from `from` whose text starts with
/// `verb` and contains `contains` (e.g. `"wagerId=w-1"`, to correlate the
/// reply with the specific delegation that triggered it). Returns `None` if
/// no matching message arrives before `timeout` — the caller must treat
/// that as "the specialist did not respond," not silently proceed.
#[allow(clippy::too_many_arguments)]
pub async fn wait_for_message(
    client: &Client,
    config: &AppConfig,
    live: &LiveSession,
    from: &str,
    verb: &str,
    contains: &str,
    timeout: Duration,
    poll_interval: Duration,
) -> Option<ThreadMessage> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if let Some(found) = poll_once(client, config, live, from, verb, contains).await {
            return Some(found);
        }
        if tokio::time::Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(poll_interval).await;
    }
}

async fn poll_once(
    client: &Client,
    config: &AppConfig,
    live: &LiveSession,
    from: &str,
    verb: &str,
    contains: &str,
) -> Option<ThreadMessage> {
    let base = config.coralos_server_url.trim_end_matches('/');
    let url = format!(
        "{base}/api/v1/local/session/{}/{}/extended",
        config.coralos_namespace, live.session_id
    );
    let response = client
        .get(url)
        .bearer_auth(&config.coralos_token)
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let body: Value = response.json().await.ok()?;

    for message in messages_of(&body) {
        let Some(sender) = str_field(&message, &["senderName", "sender", "senderId"]) else {
            continue;
        };
        let Some(text) = str_field(&message, &["text", "content"]) else {
            continue;
        };
        if sender == from && text.starts_with(verb) && text.contains(contains) {
            return Some(ThreadMessage { text });
        }
    }
    None
}

fn messages_of(state: &Value) -> Vec<Value> {
    let threads = state
        .get("threads")
        .or_else(|| state.pointer("/session/threads"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    threads
        .into_iter()
        .filter_map(|t| t.get("messages").and_then(Value::as_array).cloned())
        .flatten()
        .collect()
}

fn str_field(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|k| value.get(k).and_then(Value::as_str))
        .map(ToString::to_string)
}
