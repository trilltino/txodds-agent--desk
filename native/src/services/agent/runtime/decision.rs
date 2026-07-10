//! Applies an `AgentDecision` to the in-flight `AgentRun` record.

use serde_json::json;
use sha2::{Digest, Sha256};

use crate::domain::agent::{AgentDecision, AgentSignal};
use crate::services::coralos::protocol::MATCH_INTELLIGENCE_AGENT;
use crate::services::proof;
use crate::types::{
    AgentBid, AgentDelivery, AgentRole, AgentRun, SettlementStatus, TrackMode, TxLineProofReceipt,
    VerdictCheck, VerdictStatus, VerificationVerdict,
};

use super::super::{context, features};

pub(super) fn apply_decision_to_run(
    run: &mut AgentRun,
    context: &context::AgentContext,
    derived: &features::MarketFeatures,
    proof_receipt: &TxLineProofReceipt,
    proof_gate: &proof::ProofGateDecision,
    signal: &AgentSignal,
    decision: &AgentDecision,
) {
    let bid = AgentBid {
        agent_id: MATCH_INTELLIGENCE_AGENT.to_string(),
        role: role_for_track(context.track),
        price_sol: 0.0,
        confidence: decision.confidence,
        eta_ms: 0,
        note: format!(
            "Real Rust Coral agent: {:?} with {:?}; no seller auction or fake verifier.",
            signal.signal_type, decision.action
        ),
    };
    run.bids = vec![bid.clone()];
    run.winner = Some(bid);

    let payload = json!({
        "type": "match_intelligence_decision",
        "runId": &run.run_id,
        "fixtureId": context.event.fixture_id,
        "track": context.track,
        "signal": signal,
        "decision": decision,
        "features": derived,
        "proofGate": proof_gate,
        "proof": proof_receipt,
        "fundsMoved": false
    })
    .to_string();
    let sha256 = sha256_hex(&payload);
    run.delivery = Some(AgentDelivery {
        agent_id: MATCH_INTELLIGENCE_AGENT.to_string(),
        title: "Match Intelligence decision package".to_string(),
        payload,
        sha256: sha256.clone(),
        citations: vec![
            "Live TxLINE SSE event".to_string(),
            "Read-only txoracle validation bridge".to_string(),
        ],
        strategy: matches!(context.track, TrackMode::Trading)
            .then(|| "simulate only; no position is executed".to_string()),
        risk: Some("LLM cannot pass proof, release funds, or sign transactions".to_string()),
        fan_copy: matches!(context.track, TrackMode::Fan).then(|| decision.explanation.clone()),
    });

    let gate_verdict = proof::verdict_from_gate(proof_gate);
    run.verdict = Some(
        if proof_gate.pass && matches!(context.track, TrackMode::Settlement) {
            gate_verdict
        } else {
            VerificationVerdict {
                status: VerdictStatus::NeedsReview,
                reason: if matches!(context.track, TrackMode::Settlement) {
                    proof_gate.reason.clone()
                } else {
                    "non-settlement decision recorded; settlement remains proof-gated".to_string()
                },
                checked: vec![
                    VerdictCheck::TxlineInput,
                    VerdictCheck::Proof,
                    VerdictCheck::Policy,
                ],
            }
        },
    );

    if let Some(settlement) = run.settlement.as_mut() {
        settlement.reference = Some(format!("sha256:{sha256}"));
        settlement.status = SettlementStatus::NotStarted;
    }
}

fn role_for_track(track: TrackMode) -> AgentRole {
    match track {
        TrackMode::Settlement => AgentRole::Verifier,
        TrackMode::Trading => AgentRole::Sharp,
        TrackMode::Fan => AgentRole::Pundit,
    }
}

pub(super) fn action_summary(decision: &AgentDecision) -> String {
    format!(
        "action {:?} status {:?}",
        decision.action, decision.execution_status
    )
}

fn sha256_hex(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    hex::encode(hasher.finalize())
}
