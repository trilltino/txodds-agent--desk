//! CoralOS Console publisher.
//!
//! The Rust Match Intelligence Agent remains the brain. CoralOS is used as the
//! visible coordination bus: a named `match-intelligence-agent` participant is
//! present in a CoralOS session and the Rust runtime publishes its real messages
//! through CoralOS's puppet API.

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::config::AppConfig;
use crate::types::{AgentRun, CoralMessage, CoralVerb};

use super::protocol::{ALL_AGENTS, MATCH_INTELLIGENCE_AGENT};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoralConsolePublishResult {
    pub status: CoralConsolePublishStatus,
    pub session_id: Option<String>,
    pub thread_id: Option<String>,
    pub console_url: Option<String>,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoralConsolePublishStatus {
    Disabled,
    Published,
    Unavailable,
}

pub async fn publish_run(
    client: &Client,
    config: &AppConfig,
    run: &AgentRun,
    messages: &[CoralMessage],
) -> CoralConsolePublishResult {
    if !config.coralos_console_enabled {
        return disabled("CORALOS_CONSOLE_ENABLED=0");
    }

    let base = config.coralos_server_url.trim_end_matches('/').to_string();
    let console_url = Some(format!("{base}/ui/console"));
    let session_id = match config.coralos_session_id.clone() {
        Some(session_id) => session_id,
        None => match create_session(client, config, &base).await {
            Ok(session_id) => session_id,
            Err(err) => {
                return CoralConsolePublishResult {
                    status: CoralConsolePublishStatus::Unavailable,
                    session_id: None,
                    thread_id: None,
                    console_url,
                    note: format!("CoralOS session unavailable: {err}"),
                }
            }
        },
    };

    let thread_id = match create_thread(client, config, &base, &session_id, run).await {
        Ok(thread_id) => thread_id,
        Err(err) => {
            return CoralConsolePublishResult {
                status: CoralConsolePublishStatus::Unavailable,
                session_id: Some(session_id),
                thread_id: None,
                console_url,
                note: format!("CoralOS thread unavailable: {err}"),
            }
        }
    };

    let mut sent = 0_usize;
    for message in messages {
        if let Err(err) =
            send_message(client, config, &base, &session_id, &thread_id, message).await
        {
            return CoralConsolePublishResult {
                status: CoralConsolePublishStatus::Unavailable,
                session_id: Some(session_id),
                thread_id: Some(thread_id),
                console_url,
                note: format!("CoralOS message publish failed after {sent} messages: {err}"),
            };
        }
        sent += 1;
    }

    CoralConsolePublishResult {
        status: CoralConsolePublishStatus::Published,
        session_id: Some(session_id),
        thread_id: Some(thread_id),
        console_url,
        note: format!("published {sent} Match Intelligence messages to CoralOS Console"),
    }
}

fn disabled(note: impl Into<String>) -> CoralConsolePublishResult {
    CoralConsolePublishResult {
        status: CoralConsolePublishStatus::Disabled,
        session_id: None,
        thread_id: None,
        console_url: None,
        note: note.into(),
    }
}

async fn create_session(client: &Client, config: &AppConfig, base: &str) -> Result<String, String> {
    let response = client
        .post(format!("{base}/api/v1/local/session"))
        .bearer_auth(&config.coralos_token)
        .json(&json!({
            "agentGraphRequest": {
                "agents": ALL_AGENTS.iter().map(|name| local_agent(name)).collect::<Vec<_>>()
            },
            "namespaceProvider": {
                "type": "create_if_not_exists",
                "namespaceRequest": { "name": config.coralos_namespace }
            },
            "execution": { "mode": "immediate" }
        }))
        .send()
        .await
        .map_err(|err| err.to_string())?;

    let status = response.status();
    let body = response.text().await.map_err(|err| err.to_string())?;
    if !status.is_success() {
        return Err(format!(
            "HTTP {status}: {}",
            body.chars().take(280).collect::<String>()
        ));
    }
    let value = serde_json::from_str::<Value>(&body).map_err(|err| err.to_string())?;
    value
        .get("sessionId")
        .or_else(|| value.get("id"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| "session response did not include sessionId".to_string())
}

async fn create_thread(
    client: &Client,
    config: &AppConfig,
    base: &str,
    session_id: &str,
    run: &AgentRun,
) -> Result<String, String> {
    let url = format!(
        "{base}/api/v1/puppet/{}/{}/{}/thread",
        config.coralos_namespace, session_id, MATCH_INTELLIGENCE_AGENT
    );
    let response = client
        .post(url)
        .bearer_auth(&config.coralos_token)
        .json(&json!({
            "threadName": format!("txodds-{}-{}", run.track, run.trigger.fixture_id),
            "participantNames": ALL_AGENTS
        }))
        .send()
        .await
        .map_err(|err| err.to_string())?;

    let status = response.status();
    let body = response.text().await.map_err(|err| err.to_string())?;
    if !status.is_success() {
        return Err(format!(
            "HTTP {status}: {}",
            body.chars().take(280).collect::<String>()
        ));
    }
    let value = serde_json::from_str::<Value>(&body).map_err(|err| err.to_string())?;
    value
        .pointer("/thread/id")
        .or_else(|| value.get("threadId"))
        .or_else(|| value.get("id"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| "thread response did not include id".to_string())
}

async fn send_message(
    client: &Client,
    config: &AppConfig,
    base: &str,
    session_id: &str,
    thread_id: &str,
    message: &CoralMessage,
) -> Result<(), String> {
    let url = format!(
        "{base}/api/v1/puppet/{}/{}/{}/thread/message",
        config.coralos_namespace, session_id, MATCH_INTELLIGENCE_AGENT
    );
    let response = client
        .post(url)
        .bearer_auth(&config.coralos_token)
        .json(&json!({
            "threadId": thread_id,
            "content": coral_wire_message(message),
            "mentions": &message.to
        }))
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if response.status().is_success() {
        Ok(())
    } else {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        Err(format!(
            "HTTP {status}: {}",
            body.chars().take(280).collect::<String>()
        ))
    }
}

fn local_agent(name: &str) -> Value {
    // `runtime: "devmode"` registers the participant as externally managed:
    // coral-server adds it to the session graph (so it shows in the Console)
    // but never spawns a container for it. The Rust runtime is the brain and
    // publishes each participant's real transcript through the puppet API, so
    // no per-agent Docker image is required on any machine.
    json!({
        "id": {
            "name": name,
            "version": "0.1.0",
            "registrySourceId": { "type": "local" }
        },
        "name": name,
        "provider": { "type": "local", "runtime": "devmode" },
        "options": {
            "AGENT_NAME": { "type": "string", "value": name }
        }
    })
}

fn coral_wire_message(message: &CoralMessage) -> String {
    let text = message.text.replace('"', "'").replace('\n', " ");

    let to = message.to.join(", ");
    let run_id = message.session_id.trim_start_matches("coral-");
    format!(
        "[{}] {} → {} | {}  run={} round={}",
        wire_verb(&message.verb),
        message.from,
        to,
        text,
        run_id,
        message.round,
    )
}

fn wire_verb(verb: &CoralVerb) -> &'static str {
    match verb {
        CoralVerb::Observed => "OBSERVED",
        CoralVerb::Normalized => "NORMALIZED",
        CoralVerb::RootObserved => "ROOT_OBSERVED",
        CoralVerb::Want => "WANT",
        CoralVerb::AgentThought => "AGENT_THOUGHT",
        CoralVerb::ToolCall => "TOOL_CALL",
        CoralVerb::ToolResult => "TOOL_RESULT",
        CoralVerb::Signal => "SIGNAL",
        CoralVerb::ProofRequested => "PROOF_REQUESTED",
        CoralVerb::ProofReceived => "PROOF_RECEIVED",
        CoralVerb::ValidationSimulated => "VALIDATION_SIMULATED",
        CoralVerb::PaymentRequired => "PAYMENT_REQUIRED",
        CoralVerb::WalletConnected => "WALLET_CONNECTED",
        CoralVerb::PaymentProof => "PAYMENT_PROOF",
        CoralVerb::PaymentConfirmed => "PAYMENT_CONFIRMED",
        CoralVerb::Verified => "VERIFIED",
        CoralVerb::Settled => "SETTLED",
        CoralVerb::Evaluated => "EVALUATED",
    }
}
