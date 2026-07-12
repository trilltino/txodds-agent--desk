//! Rust MCP client for real CoralOS participants.
//!
//! Port of `pay`'s `packages/agent-runtime/src/coral/{mcp,server}.ts`
//! (verified against a live `coral-server` instance, not written from the
//! MCP spec alone — see `crates/coral-client`'s module docs on `mcp` for
//! what was actually observed). Gives a specialist agent the same shape the
//! real `pay` sellers/verifier use: connect once, then block in a
//! `wait_for_mention -> handle -> reply` loop for as long as the process
//! runs. This is what makes a participant a genuinely independent process —
//! coral-server launches it, it blocks on its own network call waiting for
//! work, and only the thread carries state between it and everyone else.

mod error;
mod mcp;
pub mod wire;

pub use error::CoralClientError;
pub use mcp::{CoralMcpAgent, CoralMcpConfig, CoralMention};

use agent_core::safety::StepCounter;

/// A specialist CoralOS participant. Implement `handle` with the agent's
/// actual decision logic; `run()` below owns connecting, the receive loop,
/// and publishing the reply — the same split as `pay`'s
/// `startCoralAgent(config, run)` / Python's `Specialist` ABC.
#[async_trait::async_trait]
pub trait Specialist: Send + Sync {
    /// CoralOS participant id. Must match the name in this agent's
    /// `coral-agent.toml` and the orchestrator's `@mention`.
    fn name(&self) -> &str;

    /// Produce a reply for one inbound mention. Called once per mention
    /// this agent receives; must not block indefinitely — safety bounds
    /// (`StepCounter`, `BudgetGuard`) belong here or in the caller's own
    /// per-call logic, consistent with every other agent in `crates/agents/*`.
    async fn handle(&self, mention: CoralMention) -> String;
}

/// Connect to CoralOS (`CORAL_CONNECTION_URL` from env, injected by
/// coral-server at container start — see `docs.coralos.ai/reference/agent`)
/// and service mentions for `specialist` until `max_steps` mentions have
/// been handled or the process is killed. There is no id-based dedup here:
/// coral-server's `wait_for_mention` is itself edge-triggered (it tracks
/// per-agent last-seen state server-side — confirmed against a live
/// instance), so every mention this loop receives is genuinely new.
pub async fn run<S: Specialist>(
    specialist: S,
    max_wait_ms: u64,
    max_steps: u64,
) -> Result<(), CoralClientError> {
    let connection_url =
        std::env::var("CORAL_CONNECTION_URL").map_err(|_| CoralClientError::NoConnectionUrl)?;
    let agent_name = specialist.name().to_string();

    tracing::info!(agent = %agent_name, url = %connection_url, "coral-client: connecting");
    let agent = CoralMcpAgent::connect(CoralMcpConfig {
        connection_url,
        agent_name: agent_name.clone(),
        version: "0.1.0".to_string(),
    })
    .await?;
    tracing::info!(agent = %agent_name, "coral-client: connected, serving mentions");

    let mut steps = StepCounter::new(max_steps);
    loop {
        let mention = match agent.wait_for_mention(max_wait_ms).await {
            Ok(Some(mention)) => mention,
            Ok(None) => continue, // timeout — keep waiting, matches pay's runLoop
            Err(err) => {
                tracing::warn!(agent = %agent_name, error = %err, "coral-client: wait error, retrying in 2s");
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                continue;
            }
        };

        let Some(thread_id) = mention.thread_id.clone() else {
            tracing::warn!(agent = %agent_name, "coral-client: mention has no threadId, dropping");
            continue;
        };

        tracing::info!(
            agent = %agent_name,
            sender = ?mention.sender,
            thread_id = %thread_id,
            "coral-client: mention received"
        );

        if steps.tick().is_err() {
            tracing::warn!(agent = %agent_name, "coral-client: max_steps reached, shutting down");
            return Ok(());
        }

        let sender = mention.sender.clone();
        let response = specialist.handle(mention).await;

        let mentions: Vec<&str> = sender.as_deref().into_iter().collect();
        if let Err(err) = agent.send_message(&response, &thread_id, &mentions).await {
            tracing::error!(agent = %agent_name, error = %err, "coral-client: failed to send reply");
        } else {
            tracing::info!(agent = %agent_name, response = %response, "coral-client: replied");
        }
    }
}
