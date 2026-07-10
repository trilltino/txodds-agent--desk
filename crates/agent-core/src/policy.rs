//! Deterministic policy for Match Intelligence Agent actions.

use crate::context::AgentContext;
use crate::domain::{
    AgentAction, AgentDecision, AgentSignal, ExecutionStatus, PolicyCheck, ProofGateDecision,
    SignalSeverity, SignalType,
};
use crate::features::MarketFeatures;
use txodds_types::{now_iso, TrackMode};

/// Apply the deterministic policy to a signal and return a decision.
#[must_use]
pub fn choose_action(
    context: &AgentContext,
    signal: &AgentSignal,
    features: &MarketFeatures,
    proof_gate: Option<&ProofGateDecision>,
    explanation: String,
) -> AgentDecision {
    let mut checks = Vec::new();
    checks.push(PolicyCheck {
        name: "live_txline_event".to_string(),
        passed: context.event.fixture_id > 0,
        detail: format!("fixture_id={}", context.event.fixture_id),
    });
    checks.push(PolicyCheck {
        name: "signal_threshold".to_string(),
        passed: signal.confidence >= 0.55,
        detail: format!("confidence={:.3}", signal.confidence),
    });

    let proof_passed = proof_gate.is_some_and(|gate| gate.pass);
    checks.push(PolicyCheck {
        name: "txoracle_proof_gate".to_string(),
        passed: proof_passed,
        detail: proof_gate
            .map_or_else(|| "proof not requested for this action".to_string(), |gate| gate.reason.clone()),
    });

    let requested_action = match (context.track, &signal.signal_type) {
        (TrackMode::Settlement, SignalType::ProofReady) => AgentAction::TriggerResolution,
        (TrackMode::Settlement, _) => AgentAction::FetchProof,
        (TrackMode::Trading, SignalType::SharpOddsMove) => AgentAction::SimulatePosition,
        (TrackMode::Fan, SignalType::ScoreEvent | SignalType::RedCardReprice) => {
            AgentAction::Notify
        }
        _ => AgentAction::Watch,
    };
    let action = match requested_action {
        AgentAction::TriggerResolution if !proof_passed => AgentAction::FetchProof,
        other => other,
    };

    let hard_blocked = checks
        .iter()
        .any(|check| !check.passed && check.name != "txoracle_proof_gate");
    let execution_status = if hard_blocked {
        ExecutionStatus::Blocked
    } else {
        ExecutionStatus::Pending
    };

    let severity_bonus = match &signal.severity {
        SignalSeverity::Critical => 0.05,
        SignalSeverity::High => 0.03,
        SignalSeverity::Medium | SignalSeverity::Low => 0.0,
    };

    AgentDecision {
        id: format!("decision-{}", uuid::Uuid::new_v4()),
        signal_id: signal.id.clone(),
        action,
        confidence: (signal.confidence + features.actionability_score + severity_bonus).min(1.0),
        policy_checks: checks,
        explanation,
        execution_status,
        created_at: now_iso(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{AgentContext, AgentThresholds};
    use crate::domain::{SignalSeverity, SignalType};
    use std::collections::BTreeMap;
    use txodds_types::{now_iso, TrackMode, TxLineEvent, TxLineEventKind};

    #[test]
    fn missing_proof_fetches_proof_instead_of_resolution() {
        let context = context(TrackMode::Settlement, TxLineEventKind::FinalWhistle);
        let signal = AgentSignal {
            id: "signal".to_string(),
            fixture_id: 1,
            source_event_id: "event".to_string(),
            signal_type: SignalType::ProofReady,
            severity: SignalSeverity::High,
            confidence: 0.9,
            features: BTreeMap::new(),
            rationale: "final".to_string(),
            created_at: now_iso(),
        };
        let gate = ProofGateDecision {
            pass: false,
            reason: "proof payload missing".to_string(),
            checked: vec![],
        };
        let decision = choose_action(
            &context,
            &signal,
            &MarketFeatures::default(),
            Some(&gate),
            "explain".to_string(),
        );
        assert_eq!(decision.action, AgentAction::FetchProof);
    }

    fn context(track: TrackMode, kind: TxLineEventKind) -> AgentContext {
        AgentContext {
            run_id: "run".to_string(),
            track,
            event: TxLineEvent {
                id: "event".to_string(),
                kind,
                fixture_id: 1,
                seq: Some(10),
                txline_ts: Some(now_iso()),
                action: None,
                confirmed: None,
                participant: None,
                period: None,
                stat_keys: vec!["1002".to_string()],
                schema_family: Some("scores".to_string()),
                title: "event".to_string(),
                body: "body".to_string(),
                ts: now_iso(),
                raw: None,
                odds: None,
                score: None,
                proof: None,
            },
            proof: None,
            thresholds: AgentThresholds {
                odds_move_trigger_pct: 5.0,
                max_devnet_spend_sol: 0.05,
            },
            recent_runs: vec![],
        }
    }
}
