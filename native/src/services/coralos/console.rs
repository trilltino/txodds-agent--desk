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

/// A real CoralOS session + thread, created eagerly (not just at the end of
/// a run) so a genuinely independent process — e.g. `proof-guard-agent` —
/// has something to actually read and reply to *during* the round, not only
/// a retrospective transcript published after the round already finished.
#[derive(Debug, Clone)]
pub struct LiveSession {
    pub session_id: String,
    pub thread_id: String,
    pub console_url: String,
}

/// Create (or reuse `config.coralos_session_id`) a real CoralOS session and
/// thread. Each call creates a fresh thread — callers that already hold a
/// [`LiveSession`] for this run should reuse it rather than calling again.
pub async fn ensure_session(
    client: &Client,
    config: &AppConfig,
    run: &AgentRun,
) -> Result<LiveSession, String> {
    ensure_session_with_override(client, config, run, None).await
}

/// Like [`ensure_session`], but `override_session_id` (when present) wins
/// over both a fresh session and `config.coralos_session_id`. Used once at
/// the top of a round to reuse the desktop app's process-lifetime CoralOS
/// session (`DesktopState.coralos_session_id`), so the Console shows one
/// durable, growing session — with a fresh thread per round — instead of a
/// new session (and four fresh Docker containers) every round, which winds
/// down within ~60s once its participants stop waiting for messages.
pub async fn ensure_session_with_override(
    client: &Client,
    config: &AppConfig,
    run: &AgentRun,
    override_session_id: Option<String>,
) -> Result<LiveSession, String> {
    let base = config.coralos_server_url.trim_end_matches('/').to_string();
    let console_url = format!("{base}/ui/console");

    let session_id = match override_session_id.or_else(|| config.coralos_session_id.clone()) {
        Some(session_id) => session_id,
        None => create_session(client, config, &base).await?,
    };
    let thread_id = create_thread(client, config, &base, &session_id, run).await?;

    Ok(LiveSession {
        session_id,
        thread_id,
        console_url,
    })
}

/// Publish one message into an already-created live session/thread
/// immediately, wrapped in the devmode/puppet transcript format. Used by
/// [`publish_run`]'s end-of-round batch for the eight participants that are
/// still puppet-narrated.
pub async fn send_live_message(
    client: &Client,
    config: &AppConfig,
    live: &LiveSession,
    message: &CoralMessage,
) -> Result<(), String> {
    let base = config.coralos_server_url.trim_end_matches('/').to_string();
    send_message(
        client,
        config,
        &base,
        &live.session_id,
        &live.thread_id,
        message,
    )
    .await
}

/// Publish a raw flat-grammar message (`VERB key=value ...`) verbatim, with
/// no devmode transcript wrapping — for delegating to a *real* registered
/// participant (e.g. `proof-guard-agent`) that parses this grammar itself
/// and would not recognise the `[VERB] from → to | text` wrapper
/// [`send_live_message`] uses for puppet-narrated participants.
pub async fn send_raw_message(
    client: &Client,
    config: &AppConfig,
    live: &LiveSession,
    content: &str,
    mentions: &[&str],
) -> Result<(), String> {
    let base = config.coralos_server_url.trim_end_matches('/').to_string();
    let url = format!(
        "{base}/api/v1/puppet/{}/{}/{}/thread/message",
        config.coralos_namespace, live.session_id, MATCH_INTELLIGENCE_AGENT
    );
    let response = client
        .post(url)
        .bearer_auth(&config.coralos_token)
        .json(&json!({
            "threadId": live.thread_id,
            "content": content,
            "mentions": mentions
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

pub async fn publish_run(
    client: &Client,
    config: &AppConfig,
    run: &AgentRun,
    messages: &[CoralMessage],
    existing: Option<LiveSession>,
) -> CoralConsolePublishResult {
    if !config.coralos_console_enabled {
        return disabled("CORALOS_CONSOLE_ENABLED=0");
    }

    let live = match existing {
        Some(live) => live,
        None => match ensure_session(client, config, run).await {
            Ok(live) => live,
            Err(err) => {
                return CoralConsolePublishResult {
                    status: CoralConsolePublishStatus::Unavailable,
                    session_id: None,
                    thread_id: None,
                    console_url: Some(format!(
                        "{}/ui/console",
                        config.coralos_server_url.trim_end_matches('/')
                    )),
                    note: format!("CoralOS session/thread unavailable: {err}"),
                }
            }
        },
    };
    let console_url = Some(live.console_url.clone());

    let mut sent = 0_usize;
    for message in messages {
        if let Err(err) = send_live_message(client, config, &live, message).await {
            return CoralConsolePublishResult {
                status: CoralConsolePublishStatus::Unavailable,
                session_id: Some(live.session_id),
                thread_id: Some(live.thread_id),
                console_url,
                note: format!("CoralOS message publish failed after {sent} messages: {err}"),
            };
        }
        sent += 1;
    }

    CoralConsolePublishResult {
        status: CoralConsolePublishStatus::Published,
        session_id: Some(live.session_id),
        thread_id: Some(live.thread_id),
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
                "agents": ALL_AGENTS.iter().map(|name| agent_graph_entry(name)).collect::<Vec<_>>()
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

/// Builds this participant's entry in `agentGraphRequest.agents`. There is
/// no "register but don't spawn" runtime in real CoralOS — decompiling a
/// live `coral-server`'s `RuntimeId` enum shows only `executable`, `docker`,
/// `function`, `prototype` exist, not the `"devmode"` this app used to send
/// (which never actually worked). Every one of the six names below is
/// therefore `runtime: "docker"`, resolved from that name's own
/// `coral-agent.toml` (discovered via `[registry] localAgents`, see
/// `coral/coral.toml`) — verified against `pay`'s own working
/// `agentGraphRequest` shape (`examples/txodds/coral/round.ts`'s `agent()`
/// helper uses the identical `{ type: "local", runtime: "docker" }` shape).
/// `PROOF_GUARD_AGENT`/`SETTLEMENT_AGENT`/`SHARP_MOVEMENT_DETECTOR_AGENT`/
/// `FAN_PUNDIT_AGENT` resolve to their own real specialist binaries;
/// `MATCH_INTELLIGENCE_AGENT`/`USER_PROXY` resolve to the shared
/// `idle-agent` image (see `crates/agents/idle-agent`) — a real, if trivial,
/// coral-server-spawned container, not a fake non-spawned registration.
fn agent_graph_entry(name: &str) -> Value {
    json!({
        "id": {
            "name": name,
            "version": "0.1.0",
            "registrySourceId": { "type": "local" }
        },
        "name": name,
        "provider": { "type": "local", "runtime": "docker" },
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
