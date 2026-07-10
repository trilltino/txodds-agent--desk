//! Coral agent registry exposed to the webview.
//!
//! The product path now has one active Coral agent: the Rust-backed Match
//! Intelligence Agent. Archived buyer/seller/verifier manifests remain under
//! `docs/legacy-coral-agents/` for historical context only.

use serde::Serialize;

// Public manifest summary returned by the `list_coral_agents` Tauri command.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoralAgentManifest {
    pub id: &'static str,
    pub display_name: &'static str,
    pub coral_role: &'static str,
    pub service: &'static str,
    pub manifest_path: &'static str,
    pub description: &'static str,
}

// Return the active agent identity used by the Match Intelligence runtime.
pub fn built_in_agents() -> Vec<CoralAgentManifest> {
    vec![
        CoralAgentManifest {
            id: "match-intelligence-agent",
            display_name: "Match Intelligence Agent",
            coral_role: "autonomous-intelligence",
            service: "txodds.match-intelligence",
            manifest_path: "coral-agents/match-intelligence-agent/coral-agent.toml",
            description: "Observes live TxLINE events, derives deterministic features, gates actions through txoracle proof validation, and publishes a real CoralOS transcript.",
        },
    ]
}
