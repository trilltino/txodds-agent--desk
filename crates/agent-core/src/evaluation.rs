//! Lightweight self-evaluation scaffolding for agent decisions.

use serde::{Deserialize, Serialize};
use txodds_types::{TxLineEvent, TxLineEventKind, ValidationSimulationStatus};

use crate::domain::{AgentAction, AgentDecision};

/// Result of a lightweight self-evaluation applied to a past decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentEvaluation {
    /// Run that produced the original decision.
    pub run_id: String,
    /// Decision being evaluated.
    pub decision_id: String,
    /// Short label for the outcome (e.g. `"correct"`, `"useful"`).
    pub outcome: String,
    /// Numeric score in `[0.0, 1.0]`.
    pub score: f64,
    /// Human-readable explanation of the evaluation.
    pub reason: String,
}

/// Compare a past decision against later events to produce an evaluation.
#[must_use]
pub fn evaluate_decision(
    run_id: &str,
    decision: &AgentDecision,
    later_events: &[TxLineEvent],
) -> Option<AgentEvaluation> {
    let proof_passed_later = later_events.iter().any(|event| {
        event
            .proof
            .as_ref()
            .is_some_and(|proof| matches!(proof.simulation_status, ValidationSimulationStatus::Passed))
    });
    let final_seen = later_events
        .iter()
        .any(|event| matches!(event.kind, TxLineEventKind::FinalWhistle));

    match &decision.action {
        AgentAction::TriggerResolution if proof_passed_later && final_seen => {
            Some(AgentEvaluation {
                run_id: run_id.to_string(),
                decision_id: decision.id.clone(),
                outcome: "correct".to_string(),
                score: 1.0,
                reason: "resolution signal matched later final event and proof pass".to_string(),
            })
        }
        AgentAction::FetchProof if proof_passed_later => Some(AgentEvaluation {
            run_id: run_id.to_string(),
            decision_id: decision.id.clone(),
            outcome: "useful".to_string(),
            score: 0.8,
            reason: "proof fetch led to later txoracle pass".to_string(),
        }),
        _ => None,
    }
}
