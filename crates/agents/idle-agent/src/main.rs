//! idle-agent — the smallest possible real CoralOS participant.
//!
//! Connects over MCP, then blocks forever. Its only purpose is to be a
//! genuine, coral-server-spawned container so the puppet REST API has a
//! valid registered target for identities that are legitimately
//! self-narrated (`match-intelligence-agent`, the desktop app narrating its
//! own actions) or human-proxied (`user-proxy`) — never a fake persona
//! standing in for an independent reasoner. Direct port of `pay`'s
//! `coral-agents/user_proxy/agent.py`.
//!
//! One image, reused by two `coral-agent.toml` manifests
//! (`crates/agents/match-intelligence-agent-proxy`,
//! `crates/agents/user-proxy`) that each declare a different `[agent] name`
//! — `AGENT_NAME` here is only for logging, coral-server resolves which
//! image to launch from the manifest's own name, not from this env var.

use coral_client::{CoralMcpAgent, CoralMcpConfig};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .json()
        .init();

    let connection_url = match std::env::var("CORAL_CONNECTION_URL") {
        Ok(url) => url,
        Err(_) => {
            tracing::error!("CORAL_CONNECTION_URL not set — coral-server must launch this participant");
            std::process::exit(1);
        }
    };
    let agent_name = std::env::var("AGENT_NAME").unwrap_or_else(|_| "idle-agent".to_string());

    tracing::info!(agent = %agent_name, url = %connection_url, "idle-agent: connecting");
    match CoralMcpAgent::connect(CoralMcpConfig {
        connection_url,
        agent_name: agent_name.clone(),
        version: "0.1.0".to_string(),
    })
    .await
    {
        Ok(_agent) => {
            tracing::info!(agent = %agent_name, "idle-agent: connected — idle, puppet API is now active for this agent");
            std::future::pending::<()>().await;
        }
        Err(err) => {
            tracing::error!(agent = %agent_name, error = %err, "idle-agent: connect failed");
            std::process::exit(1);
        }
    }
}
