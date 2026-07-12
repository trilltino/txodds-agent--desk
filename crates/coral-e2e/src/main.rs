//! coral-e2e — the Docker Compose integration test harness (TODO.md Loose
//! Ends / LOOSE-ENDS-PLAN.md §2).
//!
//! Drives the exact HTTP surface `native/` uses against a live coral-server
//! (mirrors `native/src/services/coralos/{console,thread_reader}.rs`, whose
//! request/response shapes were verified against a real server):
//!
//! 1. `POST /api/v1/local/session` — agent graph with all six CoralOS
//!    participants (`runtime: docker`), so coral-server spawns the real
//!    agent containers;
//! 2. `POST /api/v1/puppet/{ns}/{session}/match-intelligence-agent/thread`;
//! 3. puppet-POST `WAGER_PROOF_REQUESTED round=1 wagerId=w-e2e-1
//!    wager=<json>` mentioning `proof-guard-agent`;
//! 4. poll `GET /api/v1/local/session/{ns}/{session}/extended` for
//!    `proof-guard-agent`'s own `WAGER_PROOF_VERDICT … wagerId=w-e2e-1`;
//! 5. assert `passed=true` and that the round-tripped wager parses.
//!
//! Deliberately a standalone binary, not a `cargo test` target: it needs
//! Docker, the coral-server image, and every agent image built (see the run
//! book in LOOSE-ENDS-PLAN.md), so it must never gate `cargo test`.
//!
//! Environment (all optional):
//!   CORALOS_SERVER_URL  default http://localhost:5555
//!   CORAL_TOKEN         default dev
//!   CORALOS_NAMESPACE   default default
//!   E2E_TIMEOUT_SECS    default 180 (first run spawns six containers)

use std::time::Duration;

use coral_client::wire;
use serde_json::{json, Value};
use txodds_types::wager::{Selection, Wager, WagerStatus};

/// Mirror of `native/src/services/coralos/protocol.rs`'s `ALL_AGENTS` — the
/// harness can't depend on the Tauri crate, so the six CoralOS identities
/// are restated here. Keep in sync with `protocol.rs`.
const ALL_AGENTS: &[&str] = &[
    "match-intelligence-agent",
    "proof-guard-agent",
    "settlement-agent",
    "sharp-movement-detector",
    "fan-pundit-agent",
    "user-proxy",
];

const ORCHESTRATOR: &str = "match-intelligence-agent";
const PROOF_GUARD: &str = "proof-guard-agent";
const WAGER_ID: &str = "w-e2e-1";

struct Config {
    base: String,
    token: String,
    namespace: String,
    timeout: Duration,
}

impl Config {
    fn from_env() -> Self {
        let base = std::env::var("CORALOS_SERVER_URL")
            .unwrap_or_else(|_| "http://localhost:5555".to_owned())
            .trim_end_matches('/')
            .to_owned();
        Self {
            base,
            token: std::env::var("CORAL_TOKEN").unwrap_or_else(|_| "dev".to_owned()),
            namespace: std::env::var("CORALOS_NAMESPACE").unwrap_or_else(|_| "default".to_owned()),
            timeout: Duration::from_secs(
                std::env::var("E2E_TIMEOUT_SECS")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(180),
            ),
        }
    }
}

#[tokio::main]
async fn main() {
    let config = Config::from_env();
    match run(&config).await {
        Ok(summary) => {
            println!("PASS: {summary}");
        }
        Err(err) => {
            eprintln!("FAIL: {err}");
            std::process::exit(1);
        }
    }
}

async fn run(config: &Config) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("http client: {e}"))?;

    println!("coral-e2e: creating session at {} …", config.base);
    let session_id = create_session(&client, config).await?;
    println!("coral-e2e: session {session_id}; creating thread …");
    let thread_id = create_thread(&client, config, &session_id).await?;
    println!("coral-e2e: thread {thread_id}; delegating {WAGER_ID} to {PROOF_GUARD} …");

    let wager = sample_wager();
    let wager_json =
        serde_json::to_string(&wager).map_err(|e| format!("serialize wager: {e}"))?;
    let content =
        format!("WAGER_PROOF_REQUESTED round=1 wagerId={WAGER_ID} wager={wager_json}");
    send_message(&client, config, &session_id, &thread_id, &content, &[PROOF_GUARD]).await?;

    println!(
        "coral-e2e: waiting up to {}s for WAGER_PROOF_VERDICT …",
        config.timeout.as_secs()
    );
    let reply = wait_for_verdict(&client, config, &session_id)
        .await
        .ok_or_else(|| {
            format!(
                "no WAGER_PROOF_VERDICT wagerId={WAGER_ID} from {PROOF_GUARD} within {}s — \
                 is the proof-guard-agent:0.1.0 image built and did coral-server spawn it? \
                 (docker ps; docker compose -f docker-compose.coralos.yml logs coral-server)",
                config.timeout.as_secs()
            )
        })?;

    // The verdict must carry passed=true and a parseable round-tripped wager.
    let passed = wire::tok(&reply, "passed") == Some("true");
    if !passed {
        return Err(format!("verdict arrived but passed!=true: {reply}"));
    }
    let round_tripped: Wager = wire::json_val(&reply, "wager")
        .and_then(|j| serde_json::from_str(j).ok())
        .ok_or_else(|| format!("verdict wager= payload did not parse: {reply}"))?;
    if round_tripped.wager_id != WAGER_ID {
        return Err(format!(
            "verdict round-tripped the wrong wager ({}): {reply}",
            round_tripped.wager_id
        ));
    }

    Ok(format!(
        "real {PROOF_GUARD} verdict received on session {session_id}: passed=true, \
         status={:?} — CoralOS/Docker runtime verified end-to-end",
        round_tripped.status
    ))
}

/// A wager that satisfies `agent_core::proof_guard::verify`'s 5-point check
/// — same shape as proof-guard-agent's own `sample_wager` unit fixture.
fn sample_wager() -> Wager {
    Wager {
        wager_id: WAGER_ID.into(),
        fixture_id: 42,
        selection: Selection::Home,
        model_prob: 0.55,
        market_implied: 0.50,
        edge: 0.05,
        fair_odds: 1.0 / 0.55,
        stake_sol: 0.01,
        thesis: "e2e harness wager".into(),
        proof_ref: Some("txoracle:e2e-deadbeef".into()),
        status: WagerStatus::Debated,
        debate: None,
        created_at: "2026-07-11T00:00:00Z".into(),
    }
}

// ── coral-server HTTP surface (mirrors native/src/services/coralos) ──────────

async fn create_session(client: &reqwest::Client, config: &Config) -> Result<String, String> {
    let agents: Vec<Value> = ALL_AGENTS
        .iter()
        .map(|name| {
            json!({
                "id": { "name": name, "version": "0.1.0", "registrySourceId": { "type": "local" } },
                "name": name,
                "provider": { "type": "local", "runtime": "docker" },
                "options": { "AGENT_NAME": { "type": "string", "value": name } }
            })
        })
        .collect();

    let response = client
        .post(format!("{}/api/v1/local/session", config.base))
        .bearer_auth(&config.token)
        .json(&json!({
            "agentGraphRequest": { "agents": agents },
            "namespaceProvider": {
                "type": "create_if_not_exists",
                "namespaceRequest": { "name": config.namespace }
            },
            "execution": { "mode": "immediate" }
        }))
        .send()
        .await
        .map_err(|e| format!("create session: {e}"))?;

    let status = response.status();
    let body = response.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("create session HTTP {status}: {}", truncate(&body)));
    }
    let value: Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    value
        .get("sessionId")
        .or_else(|| value.get("id"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| format!("session response had no sessionId: {}", truncate(&body)))
}

async fn create_thread(
    client: &reqwest::Client,
    config: &Config,
    session_id: &str,
) -> Result<String, String> {
    let response = client
        .post(format!(
            "{}/api/v1/puppet/{}/{}/{}/thread",
            config.base, config.namespace, session_id, ORCHESTRATOR
        ))
        .bearer_auth(&config.token)
        .json(&json!({
            "threadName": "coral-e2e-proof-guard",
            "participantNames": ALL_AGENTS
        }))
        .send()
        .await
        .map_err(|e| format!("create thread: {e}"))?;

    let status = response.status();
    let body = response.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("create thread HTTP {status}: {}", truncate(&body)));
    }
    let value: Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    value
        .pointer("/thread/id")
        .or_else(|| value.get("threadId"))
        .or_else(|| value.get("id"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| format!("thread response had no id: {}", truncate(&body)))
}

async fn send_message(
    client: &reqwest::Client,
    config: &Config,
    session_id: &str,
    thread_id: &str,
    content: &str,
    mentions: &[&str],
) -> Result<(), String> {
    let response = client
        .post(format!(
            "{}/api/v1/puppet/{}/{}/{}/thread/message",
            config.base, config.namespace, session_id, ORCHESTRATOR
        ))
        .bearer_auth(&config.token)
        .json(&json!({
            "threadId": thread_id,
            "content": content,
            "mentions": mentions
        }))
        .send()
        .await
        .map_err(|e| format!("send message: {e}"))?;
    let status = response.status();
    if status.is_success() {
        Ok(())
    } else {
        let body = response.text().await.unwrap_or_default();
        Err(format!("send message HTTP {status}: {}", truncate(&body)))
    }
}

async fn wait_for_verdict(
    client: &reqwest::Client,
    config: &Config,
    session_id: &str,
) -> Option<String> {
    let deadline = tokio::time::Instant::now() + config.timeout;
    let contains = format!("wagerId={WAGER_ID}");
    loop {
        if let Some(text) = poll_once(client, config, session_id, &contains).await {
            return Some(text);
        }
        if tokio::time::Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(1500)).await;
    }
}

async fn poll_once(
    client: &reqwest::Client,
    config: &Config,
    session_id: &str,
    contains: &str,
) -> Option<String> {
    let response = client
        .get(format!(
            "{}/api/v1/local/session/{}/{}/extended",
            config.base, config.namespace, session_id
        ))
        .bearer_auth(&config.token)
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let body: Value = response.json().await.ok()?;

    let threads = body
        .get("threads")
        .or_else(|| body.pointer("/session/threads"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for message in threads
        .into_iter()
        .filter_map(|t| t.get("messages").and_then(Value::as_array).cloned())
        .flatten()
    {
        let sender = ["senderName", "sender", "senderId"]
            .iter()
            .find_map(|k| message.get(k).and_then(Value::as_str));
        let text = ["text", "content"]
            .iter()
            .find_map(|k| message.get(k).and_then(Value::as_str));
        if let (Some(sender), Some(text)) = (sender, text) {
            if sender == PROOF_GUARD
                && text.starts_with("WAGER_PROOF_VERDICT")
                && text.contains(contains)
            {
                return Some(text.to_owned());
            }
        }
    }
    None
}

fn truncate(body: &str) -> String {
    body.chars().take(280).collect()
}
