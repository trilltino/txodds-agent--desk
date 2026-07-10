//! Proof receipt and deterministic proof-gate helpers.

use serde_json::json;

use crate::types::{
    AgentRun, TxLineProofReceipt, ValidationSimulationStatus, VerdictCheck, VerdictStatus,
    VerificationVerdict,
};

mod validation;

pub use validation::ValidationBridge;

// Re-export from agent-core so all crates share the same concrete type.
pub use agent_core::domain::ProofGateDecision;

pub fn receipt_for_run(run: &AgentRun) -> TxLineProofReceipt {
    if let Some(proof) = run.trigger.proof.clone() {
        return proof;
    }

    let stat_keys = if run.trigger.stat_keys.is_empty() {
        fallback_stat_keys(run)
    } else {
        run.trigger.stat_keys.clone()
    };

    TxLineProofReceipt {
        fixture_id: run.trigger.fixture_id,
        seq: run.trigger.seq,
        stat_key: None,
        stat_keys,
        txline_ts: run.trigger.txline_ts.clone(),
        epoch_day: run
            .trigger
            .txline_ts
            .as_deref()
            .and_then(epoch_day_from_iso),
        merkle_root: None,
        stat_proof_hash: None,
        root_pda: None,
        txline_program: None,
        root_observed_slot: None,
        proof_present: false,
        root_present: false,
        simulation_status: ValidationSimulationStatus::NotStarted,
        verified: false,
        note: "TxLINE proof request queued; no proof/root payload is available yet".to_string(),
        raw: Some(json!({
            "source": "local-proof-builder",
            "triggerEventId": run.trigger.id,
            "schemaFamily": run.trigger.schema_family
        })),
    }
}

pub fn gate_receipt(run: &AgentRun, receipt: &TxLineProofReceipt) -> ProofGateDecision {
    let mut checked = vec![
        "fixture_id".to_string(),
        "stat_keys".to_string(),
        "proof_payload".to_string(),
        "root_observed".to_string(),
        "simulation_status".to_string(),
    ];

    if receipt.fixture_id != run.trigger.fixture_id {
        return ProofGateDecision {
            pass: false,
            reason: "proof fixture does not match trigger fixture".to_string(),
            checked,
        };
    }
    if receipt.stat_keys.is_empty() {
        return ProofGateDecision {
            pass: false,
            reason: "proof receipt has no stat keys".to_string(),
            checked,
        };
    }
    if !receipt.proof_present {
        return ProofGateDecision {
            pass: false,
            reason: "proof payload missing".to_string(),
            checked,
        };
    }
    if !receipt.root_present {
        return ProofGateDecision {
            pass: false,
            reason: "txoracle root not observed".to_string(),
            checked,
        };
    }
    checked.push("deterministic_predicate".to_string());
    let pass = matches!(
        receipt.simulation_status,
        ValidationSimulationStatus::Passed
    );
    ProofGateDecision {
        pass,
        reason: if pass {
            "TxLINE proof matched observed txoracle root and validation simulation passed"
                .to_string()
        } else {
            "validation simulation did not pass".to_string()
        },
        checked,
    }
}

#[allow(dead_code)] // Used when proof gate fully replaces the compatibility verifier.
pub fn verdict_from_gate(gate: &ProofGateDecision) -> VerificationVerdict {
    VerificationVerdict {
        status: if gate.pass {
            VerdictStatus::Pass
        } else {
            VerdictStatus::NeedsReview
        },
        reason: gate.reason.clone(),
        checked: vec![
            VerdictCheck::TxlineInput,
            VerdictCheck::Hash,
            VerdictCheck::Proof,
            VerdictCheck::Policy,
        ],
    }
}

fn fallback_stat_keys(run: &AgentRun) -> Vec<String> {
    match run.trigger.kind {
        crate::types::TxLineEventKind::Goal | crate::types::TxLineEventKind::ScoreUpdate => {
            vec!["score.home".to_string(), "score.away".to_string()]
        }
        crate::types::TxLineEventKind::OddsMove | crate::types::TxLineEventKind::OddsUpdate => {
            vec!["odds.stream".to_string()]
        }
        _ => vec!["fixture.context".to_string()],
    }
}

fn epoch_day_from_iso(value: &str) -> Option<u32> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|dt| (dt.timestamp() / 86_400) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{now_iso, AgentRun, TrackMode, TxLineEvent, TxLineEventKind};

    #[test]
    fn missing_proof_needs_review() {
        let run = AgentRun {
            run_id: "run".to_string(),
            track: TrackMode::Settlement,
            trigger: test_event(),
            bids: vec![],
            winner: None,
            delivery: None,
            verdict: None,
            settlement: None,
            timeline: vec![],
        };
        let receipt = receipt_for_run(&run);
        let gate = gate_receipt(&run, &receipt);
        assert!(!gate.pass);
        assert_eq!(gate.reason, "proof payload missing");
    }

    fn test_event() -> TxLineEvent {
        TxLineEvent {
            id: "test-live-event".to_string(),
            kind: TxLineEventKind::ScoreUpdate,
            fixture_id: 1,
            seq: Some(10),
            txline_ts: Some(now_iso()),
            action: Some("ScoreUpdate".to_string()),
            confirmed: Some(true),
            participant: None,
            period: None,
            stat_keys: vec!["1002".to_string()],
            schema_family: Some("scores".to_string()),
            title: "Test score update".to_string(),
            body: "Unit-test live event fixture".to_string(),
            ts: now_iso(),
            raw: None,
            odds: None,
            score: None,
            proof: None,
        }
    }
}
