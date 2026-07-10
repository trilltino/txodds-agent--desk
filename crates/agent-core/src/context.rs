//! Per-event context for the Match Intelligence Agent.

use serde::{Deserialize, Serialize};
use txodds_types::{AgentRun, TrackMode, TxLineEvent, TxLineProofReceipt};

/// Everything the agent needs to evaluate a single incoming event.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentContext {
    /// Unique identifier for this agent run.
    pub run_id: String,
    /// Which track (Settlement, Trading, Fan) this run belongs to.
    pub track: TrackMode,
    /// The `TxLINE` event that triggered this run.
    pub event: TxLineEvent,
    /// Optional proof receipt attached to the event.
    pub proof: Option<TxLineProofReceipt>,
    /// Operator-configured thresholds for this session.
    pub thresholds: AgentThresholds,
    /// Summaries of the most recent prior runs (max 20).
    pub recent_runs: Vec<RunSummary>,
}

/// Operator-configured thresholds that govern agent behaviour.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentThresholds {
    /// Minimum percentage odds movement required to trigger a signal.
    pub odds_move_trigger_pct: f64,
    /// Maximum SOL the agent may spend per session on devnet.
    pub max_devnet_spend_sol: f64,
}

/// Abbreviated summary of a prior agent run, used for working-memory context.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    /// Run identifier.
    pub run_id: String,
    /// `TxLINE` fixture the run processed.
    pub fixture_id: u64,
    /// Track mode of the run.
    pub track: TrackMode,
    /// ISO-8601 timestamp when the run was created.
    pub created_at: String,
}

/// Build a context without touching `AppConfig` — the caller passes the two
/// threshold values directly so agent-core stays config-independent.
#[must_use]
pub fn build_context(
    thresholds: AgentThresholds,
    track: TrackMode,
    event: TxLineEvent,
    proof: Option<TxLineProofReceipt>,
    recent_runs: Vec<AgentRun>,
) -> AgentContext {
    AgentContext {
        run_id: format!("{track}-{}-{}", event.id, uuid::Uuid::new_v4()),
        track,
        event,
        proof,
        thresholds,
        recent_runs: recent_runs
            .into_iter()
            .take(20)
            .map(|run| RunSummary {
                run_id: run.run_id,
                fixture_id: run.trigger.fixture_id,
                track: run.track,
                created_at: run
                    .timeline
                    .first()
                    .map_or_else(txodds_types::now_iso, |entry| entry.at.clone()),
            })
            .collect(),
    }
}
