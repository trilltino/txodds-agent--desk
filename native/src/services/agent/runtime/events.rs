//! Coral message/trace emission and timeline bookkeeping shared by the
//! Match Intelligence Agent round.

use serde_json::Value;
use tauri::{AppHandle, Emitter};

use crate::event_bus;
use crate::services::coralos::protocol::message;
use crate::types::{
    now_iso, AgentRun, AgentTraceEvent, AgentTracePhase, CoralMessage, CoralSession, CoralVerb,
    TimelineEntry, TxLineEvent, TxLineEventKind, TxLineProofReceipt,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn emit_message(
    app: &AppHandle,
    session: &CoralSession,
    messages: &mut Vec<CoralMessage>,
    round: u64,
    from: impl Into<String>,
    to: Vec<&str>,
    verb: CoralVerb,
    text: impl Into<String>,
    payload: Option<Value>,
) {
    let message = message(session, round, from, to, verb, text, payload);
    let _ = app.emit(event_bus::CORAL_MESSAGE, &message);
    messages.push(message);
}

pub(super) fn emit_trace(
    app: &AppHandle,
    trace: &mut Vec<AgentTraceEvent>,
    run_id: &str,
    round: u64,
    phase: AgentTracePhase,
    summary: impl Into<String>,
    payload: Option<Value>,
) {
    let event = AgentTraceEvent {
        id: format!("trace-{}", uuid::Uuid::new_v4()),
        run_id: run_id.to_string(),
        round,
        phase,
        summary: summary.into(),
        payload,
        ts: now_iso(),
    };
    let _ = app.emit(event_bus::AGENT_TRACE, &event);
    trace.push(event);
}

pub(super) fn append_timeline(run: &mut AgentRun, label: impl Into<String>, detail: impl Into<String>) {
    run.timeline.push(TimelineEntry {
        at: now_iso(),
        label: label.into(),
        detail: detail.into(),
    });
}

pub(super) fn proof_event(run: &AgentRun, proof: &TxLineProofReceipt) -> TxLineEvent {
    TxLineEvent {
        id: format!("proof-{}-{}", run.run_id, uuid::Uuid::new_v4()),
        kind: TxLineEventKind::ProofReceived,
        fixture_id: proof.fixture_id,
        seq: proof.seq,
        txline_ts: proof.txline_ts.clone(),
        action: Some("ProofReceived".to_string()),
        confirmed: Some(proof.verified),
        participant: None,
        period: None,
        stat_keys: proof.stat_keys.clone(),
        schema_family: Some("proof".to_string()),
        title: if proof.verified {
            "TxLINE proof verified".to_string()
        } else {
            "TxLINE proof pending".to_string()
        },
        body: proof.note.clone(),
        ts: now_iso(),
        raw: proof.raw.clone(),
        odds: None,
        score: None,
        proof: Some(proof.clone()),
    }
}
