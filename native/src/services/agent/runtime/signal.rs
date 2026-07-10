//! Deterministic signal derivation from live TxLINE events.

use std::collections::BTreeMap;

use serde_json::json;

use crate::domain::agent::{AgentSignal, SignalSeverity, SignalType};
use crate::types::{now_iso, TxLineEventKind};

use super::super::{context, features};

pub(super) fn build_signal(
    context: &context::AgentContext,
    derived: &features::MarketFeatures,
) -> Option<AgentSignal> {
    let signal_type = match &context.event.kind {
        TxLineEventKind::OddsMove | TxLineEventKind::OddsUpdate => SignalType::SharpOddsMove,
        TxLineEventKind::Goal | TxLineEventKind::ScoreUpdate => SignalType::ScoreEvent,
        TxLineEventKind::RedCard => SignalType::RedCardReprice,
        TxLineEventKind::FinalWhistle | TxLineEventKind::ProofReceived => SignalType::ProofReady,
        _ => return None,
    };

    let severity = if derived.severity_score >= 0.85 {
        SignalSeverity::Critical
    } else if derived.severity_score >= 0.70 {
        SignalSeverity::High
    } else if derived.severity_score >= 0.55 {
        SignalSeverity::Medium
    } else {
        SignalSeverity::Low
    };

    let mut measured = BTreeMap::new();
    measured.insert("severityScore".to_string(), json!(derived.severity_score));
    measured.insert(
        "actionabilityScore".to_string(),
        json!(derived.actionability_score),
    );
    measured.insert("proofPresent".to_string(), json!(derived.proof_present));
    measured.insert("rootPresent".to_string(), json!(derived.root_present));
    measured.insert("txoraclePassed".to_string(), json!(derived.txoracle_passed));
    if let Some(probability) = derived.best_implied_probability {
        measured.insert("bestImpliedProbability".to_string(), json!(probability));
    }

    Some(AgentSignal {
        id: format!("signal-{}", uuid::Uuid::new_v4()),
        fixture_id: context.event.fixture_id,
        source_event_id: context.event.id.clone(),
        signal_type,
        severity,
        confidence: derived.severity_score.max(derived.actionability_score),
        features: measured,
        rationale: derived.reasons.join("; "),
        created_at: now_iso(),
    })
}

pub(super) fn feature_summary(derived: &features::MarketFeatures) -> String {
    format!(
        "severity={:.2} actionability={:.2} proof={} root={} txoracle={}",
        derived.severity_score,
        derived.actionability_score,
        derived.proof_present,
        derived.root_present,
        derived.txoracle_passed
    )
}

pub(super) fn signal_summary(signal: &AgentSignal) -> String {
    format!(
        "{:?} for fixture {} confidence {:.2}",
        signal.signal_type, signal.fixture_id, signal.confidence
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{now_iso, TrackMode, TxLineEvent};

    #[test]
    fn low_severity_fixture_does_not_emit_signal() {
        let event = test_event(TxLineEventKind::Fixture);
        let context = context::AgentContext {
            run_id: "run".to_string(),
            track: TrackMode::Fan,
            event,
            proof: None,
            thresholds: context::AgentThresholds {
                odds_move_trigger_pct: 5.0,
                max_devnet_spend_sol: 0.05,
            },
            recent_runs: vec![],
        };
        let derived = features::derive_features(&context.event);
        assert!(build_signal(&context, &derived).is_none());
    }

    fn test_event(kind: TxLineEventKind) -> TxLineEvent {
        TxLineEvent {
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
        }
    }
}
