//! Real Match Intelligence Agent runtime.
//!
//! A live TxLINE event drives the run directly; this module is the sole
//! entrypoint for `run_agent_round`. Submodules split the round's
//! responsibilities: `signal` derives the deterministic trigger signal,
//! `decision` applies the chosen action to the run record, `handoff` picks
//! and acknowledges the track specialist, `events` emits Coral
//! messages/traces and timeline entries, and `persistence` writes the
//! completed round to the ledger.

use serde_json::json;
use tauri::{AppHandle, Emitter};

use crate::error::AppError;
use crate::event_bus;
use crate::services::coralos::protocol::{
    FAN_PUNDIT_AGENT, MATCH_INTELLIGENCE_AGENT, PROOF_GUARD_AGENT, USER_PROXY,
};
use crate::services::{coralos, llm};
use crate::state::DesktopState;
use crate::types::{
    now_iso, AgentRun, AgentTracePhase, CoralVerb, MarketRoundEvent, SettlementReceipt,
    SettlementStatus, TimelineEntry, TrackMode, TxLineEvent, VerdictCheck, VerdictStatus,
    VerificationVerdict,
};

use super::{authority, context, evaluation, features, policy, pundit_agent, tools, wager_agent};

mod decision;
mod events;
mod handoff;
mod persistence;
mod signal;

use decision::{action_summary, apply_decision_to_run};
use events::{append_timeline, emit_message, emit_trace, proof_event};
use handoff::{specialist_ack, specialist_for_track, wager_ruling_payload};
use persistence::persist_run;
use signal::{build_signal, feature_summary, signal_summary};

pub async fn run_match_intelligence_round(
    app: AppHandle,
    state: &DesktopState,
    trigger: TxLineEvent,
    track: TrackMode,
) -> Result<AgentRun, AppError> {
    let recent_runs = {
        let ledger = state
            .ledger
            .lock()
            .map_err(|_| AppError::Task("ledger lock poisoned".to_string()))?;
        ledger.list_runs().unwrap_or_default()
    };
    let mut context = context::build_context(
        context::AgentThresholds {
            odds_move_trigger_pct: state.config.odds_move_trigger_pct,
            max_devnet_spend_sol: state.config.max_devnet_spend_sol,
        },
        track,
        trigger,
        None,
        recent_runs,
    );
    let session =
        coralos::protocol::start_session(&context.run_id, context.event.fixture_id, context.track);
    let mut run = empty_run(&context);
    let mut messages = Vec::new();
    let mut trace = Vec::new();
    let mut round = 1_u64;

    let _ = app.emit(event_bus::CORAL_SESSION, &session);
    emit_message(
        &app,
        &session,
        &mut messages,
        round,
        "txline-ingest",
        vec![MATCH_INTELLIGENCE_AGENT],
        CoralVerb::Observed,
        format!(
            "observed {:?} for fixture {}",
            context.event.kind, context.event.fixture_id
        ),
        Some(json!({
            "eventId": &context.event.id,
            "fixtureId": context.event.fixture_id,
            "kind": &context.event.kind,
            "seq": context.event.seq
        })),
    );
    emit_trace(
        &app,
        &mut trace,
        &run.run_id,
        round,
        AgentTracePhase::Observe,
        "live TxLINE event observed",
        Some(json!({ "eventId": &context.event.id, "fixtureId": context.event.fixture_id })),
    );
    append_timeline(&mut run, "OBSERVE", "live TxLINE event observed");

    round += 1;
    emit_message(
        &app,
        &session,
        &mut messages,
        round,
        "txline-normalizer",
        vec![MATCH_INTELLIGENCE_AGENT],
        CoralVerb::Normalized,
        format!(
            "normalized fixture {} seq {} with {} stat keys",
            context.event.fixture_id,
            context
                .event
                .seq
                .map(|seq| seq.to_string())
                .unwrap_or_else(|| "n/a".to_string()),
            context.event.stat_keys.len()
        ),
        Some(json!({
            "fixtureId": context.event.fixture_id,
            "seq": context.event.seq,
            "statKeys": &context.event.stat_keys,
            "schemaFamily": &context.event.schema_family
        })),
    );

    let mut derived = features::derive_features(&context.event);
    emit_trace(
        &app,
        &mut trace,
        &run.run_id,
        round,
        AgentTracePhase::Derive,
        "market features derived",
        Some(json!({ "features": derived })),
    );
    append_timeline(&mut run, "FEATURES", "market features derived");

    round += 1;
    emit_message(
        &app,
        &session,
        &mut messages,
        round,
        USER_PROXY,
        vec![MATCH_INTELLIGENCE_AGENT],
        CoralVerb::Want,
        format!(
            "WANT txodds.match-intelligence fixture:{} track:{}",
            context.event.fixture_id, context.track
        ),
        Some(json!({ "runId": &run.run_id, "track": context.track })),
    );

    round += 1;
    emit_message(
        &app,
        &session,
        &mut messages,
        round,
        MATCH_INTELLIGENCE_AGENT,
        vec!["feature-extractor"],
        CoralVerb::ToolCall,
        "derive deterministic market features",
        Some(json!({ "tool": "feature_extractor", "fixtureId": context.event.fixture_id })),
    );
    emit_message(
        &app,
        &session,
        &mut messages,
        round,
        "feature-extractor",
        vec![MATCH_INTELLIGENCE_AGENT],
        CoralVerb::ToolResult,
        feature_summary(&derived),
        Some(json!({ "features": &derived })),
    );
    emit_trace(
        &app,
        &mut trace,
        &run.run_id,
        round,
        AgentTracePhase::ToolResult,
        feature_summary(&derived),
        Some(json!({ "features": &derived })),
    );

    round += 1;
    emit_message(
        &app,
        &session,
        &mut messages,
        round,
        MATCH_INTELLIGENCE_AGENT,
        vec![PROOF_GUARD_AGENT],
        CoralVerb::ProofRequested,
        format!(
            "request TxLINE txoracle proof for fixture {} seq {}",
            context.event.fixture_id,
            context
                .event
                .seq
                .map(|seq| seq.to_string())
                .unwrap_or_else(|| "latest".to_string())
        ),
        Some(json!({
            "fixtureId": context.event.fixture_id,
            "seq": context.event.seq,
            "statKeys": &context.event.stat_keys
        })),
    );
    emit_trace(
        &app,
        &mut trace,
        &run.run_id,
        round,
        AgentTracePhase::ToolCall,
        "txoracle proof requested",
        Some(json!({ "tool": "txoracle_validation" })),
    );
    let (proof_receipt, proof_gate) =
        tools::request_proof(&state.validation_bridge, &state.client, &state.config, &run).await;
    context.proof = Some(proof_receipt.clone());
    context.event.proof = Some(proof_receipt.clone());
    run.trigger.proof = Some(proof_receipt.clone());
    derived = features::derive_features(&context.event);
    append_timeline(
        &mut run,
        "PROOF_GATE",
        format!(
            "{}: {}",
            if proof_gate.pass {
                "pass"
            } else {
                "needs_review"
            },
            proof_gate.reason
        ),
    );
    emit_message(
        &app,
        &session,
        &mut messages,
        round,
        PROOF_GUARD_AGENT,
        vec![MATCH_INTELLIGENCE_AGENT],
        CoralVerb::ProofReceived,
        proof_receipt.note.clone(),
        Some(json!({ "receipt": &proof_receipt, "gate": &proof_gate })),
    );
    emit_message(
        &app,
        &session,
        &mut messages,
        round,
        PROOF_GUARD_AGENT,
        vec![MATCH_INTELLIGENCE_AGENT],
        CoralVerb::ValidationSimulated,
        format!("txoracle simulation {:?}", proof_receipt.simulation_status),
        Some(json!({
            "status": &proof_receipt.simulation_status,
            "verified": proof_receipt.verified,
            "gatePass": proof_gate.pass
        })),
    );
    emit_trace(
        &app,
        &mut trace,
        &run.run_id,
        round,
        AgentTracePhase::Proof,
        proof_receipt.note.clone(),
        Some(json!({ "receipt": &proof_receipt, "gate": &proof_gate })),
    );
    let _ = app.emit(event_bus::WEB3_PROOF_RECEIPT, &proof_receipt);
    let _ = app.emit(
        event_bus::VALIDATION_STATUS,
        json!({
            "runId": &run.run_id,
            "status": &proof_receipt.simulation_status,
            "verified": proof_receipt.verified,
            "note": &proof_receipt.note
        }),
    );
    let _ = app.emit(event_bus::TXLINE_EVENT, proof_event(&run, &proof_receipt));

    let llm_response = explain_decision(&state.client, &state.config, &context, &derived).await;
    round += 1;
    emit_message(
        &app,
        &session,
        &mut messages,
        round,
        MATCH_INTELLIGENCE_AGENT,
        vec![USER_PROXY],
        CoralVerb::AgentThought,
        llm_response.text.clone(),
        Some(json!({
            "llm": {
                "provider": &llm_response.provider,
                "model": &llm_response.model,
                "used": llm_response.used,
                "reason": &llm_response.reason,
                "traceEnabled": state.config.llm_trace
            },
            "affectedFunds": false
        })),
    );
    emit_trace(
        &app,
        &mut trace,
        &run.run_id,
        round,
        AgentTracePhase::LlmReasoning,
        if llm_response.used {
            "Venice explanation generated"
        } else {
            "deterministic explanation used"
        },
        Some(json!({ "llm": &llm_response })),
    );

    // ── Fundamentals wager proposal (rig-venice ROADMAP.md Phase 4) ──────────
    //
    // Additive: runs alongside the severity/actionability signal pipeline
    // below, does not replace or gate it. Only produces a ruling when the
    // event carries a complete 1X2 market; the Rust Authority
    // (`authority::adjudicate`) re-derives edge/stake and is the only thing
    // that decides whether this is actually a bet, never the LLM.
    let wager_policy = authority::AuthorityPolicy::from_max_spend(state.config.max_devnet_spend_sol);
    let wager_proof_ref = proof_gate
        .pass
        .then(|| {
            proof_receipt
                .stat_proof_hash
                .clone()
                .unwrap_or_else(|| format!("txoracle:{}", proof_receipt.note))
        });
    let wager_outcome = wager_agent::propose_wager(&context.event, wager_proof_ref, wager_policy).await;
    round += 1;
    emit_message(
        &app,
        &session,
        &mut messages,
        round,
        MATCH_INTELLIGENCE_AGENT,
        vec![USER_PROXY],
        CoralVerb::ToolResult,
        wager_outcome.narrative.clone(),
        Some(wager_ruling_payload(wager_outcome.ruling.as_ref())),
    );
    emit_trace(
        &app,
        &mut trace,
        &run.run_id,
        round,
        AgentTracePhase::ToolResult,
        wager_outcome.narrative.clone(),
        Some(wager_ruling_payload(wager_outcome.ruling.as_ref())),
    );
    if let Some(ruling) = &wager_outcome.ruling {
        append_timeline(
            &mut run,
            "WAGER",
            format!("{:?}: {}", ruling.wager.status, ruling.reason),
        );

        // ── Fan-pundit narrative reaction (rig-venice ROADMAP.md Phase 5) ────
        //
        // Independent second Venice call that endorses/challenges the wager
        // the fundamentals agent just proposed. Its nudge is re-adjudicated
        // through the same Authority — it never sets stake or status
        // directly.
        let pundit_outcome = pundit_agent::react_to_wager(ruling, wager_policy).await;
        round += 1;
        emit_message(
            &app,
            &session,
            &mut messages,
            round,
            FAN_PUNDIT_AGENT,
            vec![MATCH_INTELLIGENCE_AGENT, USER_PROXY],
            CoralVerb::ToolResult,
            pundit_outcome.narrative.clone(),
            Some(wager_ruling_payload(pundit_outcome.updated_ruling.as_ref())),
        );
        emit_trace(
            &app,
            &mut trace,
            &run.run_id,
            round,
            AgentTracePhase::ToolResult,
            pundit_outcome.narrative.clone(),
            None,
        );
        if let Some(updated) = &pundit_outcome.updated_ruling {
            append_timeline(
                &mut run,
                "PUNDIT",
                format!("{:?}: {}", updated.wager.status, updated.reason),
            );
        }
    }

    let maybe_signal = build_signal(&context, &derived);
    let mut maybe_decision = None;
    if let Some(signal) = maybe_signal {
        {
            let ledger = state
                .ledger
                .lock()
                .map_err(|_| AppError::Task("ledger lock poisoned".to_string()))?;
            let _ = ledger.insert_agent_signal(&run.run_id, &signal);
        }
        round += 1;
        emit_message(
            &app,
            &session,
            &mut messages,
            round,
            MATCH_INTELLIGENCE_AGENT,
            vec![USER_PROXY],
            CoralVerb::Signal,
            signal_summary(&signal),
            Some(json!({ "signal": &signal })),
        );
        let _ = app.emit(
            event_bus::AGENT_SIGNAL,
            messages.last().expect("signal message emitted"),
        );

        let decision = policy::choose_action(
            &context,
            &signal,
            &derived,
            Some(&proof_gate),
            llm_response.text.clone(),
        );
        apply_decision_to_run(
            &mut run,
            &context,
            &derived,
            &proof_receipt,
            &proof_gate,
            &signal,
            &decision,
        );
        append_timeline(
            &mut run,
            "DECISION",
            format!("{:?} -> {:?}", signal.signal_type, decision.action),
        );
        emit_trace(
            &app,
            &mut trace,
            &run.run_id,
            round,
            AgentTracePhase::Decision,
            decision.explanation.clone(),
            Some(json!({
                "signal": &signal,
                "decision": &decision,
                "proofGate": &proof_gate
            })),
        );
        emit_trace(
            &app,
            &mut trace,
            &run.run_id,
            round,
            AgentTracePhase::Action,
            action_summary(&decision),
            Some(json!({ "decision": &decision })),
        );
        emit_message(
            &app,
            &session,
            &mut messages,
            round,
            MATCH_INTELLIGENCE_AGENT,
            vec![USER_PROXY],
            CoralVerb::ToolResult,
            action_summary(&decision),
            Some(json!({ "decision": &decision })),
        );
        // --- multi-agent handoff: delegate to the track specialist ---
        let specialist = specialist_for_track(context.track);
        round += 1;
        emit_message(
            &app,
            &session,
            &mut messages,
            round,
            MATCH_INTELLIGENCE_AGENT,
            vec![specialist],
            CoralVerb::ToolCall,
            format!(
                "DELEGATE {:?} decision to {} | confidence={:.2} proofGate={}",
                decision.action,
                specialist,
                decision.confidence,
                proof_gate.pass,
            ),
            Some(json!({
                "specialist": specialist,
                "track": context.track,
                "signal": &signal,
                "decision": &decision,
                "proofGate": &proof_gate
            })),
        );
        emit_message(
            &app,
            &session,
            &mut messages,
            round,
            specialist,
            vec![MATCH_INTELLIGENCE_AGENT, USER_PROXY],
            CoralVerb::ToolResult,
            specialist_ack(context.track, &decision.explanation),
            Some(json!({
                "specialist": specialist,
                "status": "acknowledged",
                "track": context.track
            })),
        );
        emit_trace(
            &app,
            &mut trace,
            &run.run_id,
            round,
            AgentTracePhase::Action,
            format!("specialist {} acknowledged handoff", specialist),
            Some(json!({ "specialist": specialist })),
        );
        append_timeline(
            &mut run,
            "HANDOFF",
            format!("{} → {}", MATCH_INTELLIGENCE_AGENT, specialist),
        );

        maybe_decision = Some(decision);
    } else {
        run.verdict = Some(VerificationVerdict {
            status: VerdictStatus::NeedsReview,
            reason: "event stayed below autonomous signal threshold".to_string(),
            checked: vec![
                VerdictCheck::TxlineInput,
                VerdictCheck::Policy,
            ],
        });
        append_timeline(&mut run, "DECISION", "no actionable signal emitted");
    }

    let queued_evaluation = maybe_decision
        .as_ref()
        .and_then(|decision| evaluation::evaluate_decision(&run.run_id, decision, &[]));
    round += 1;
    emit_message(
        &app,
        &session,
        &mut messages,
        round,
        MATCH_INTELLIGENCE_AGENT,
        vec![USER_PROXY],
        CoralVerb::Evaluated,
        "evaluation queued until later live TxLINE updates arrive",
        Some(json!({
            "status": "queued",
            "windowSecs": 900,
            "currentEvaluation": queued_evaluation
        })),
    );
    let _ = app.emit(
        event_bus::AGENT_EVALUATION,
        messages.last().expect("evaluation message emitted"),
    );
    emit_trace(
        &app,
        &mut trace,
        &run.run_id,
        round,
        AgentTracePhase::Evaluation,
        "evaluation queued",
        Some(json!({
            "status": "queued",
            "windowSecs": 900,
            "currentEvaluation": queued_evaluation
        })),
    );

    let console =
        coralos::console::publish_run(&state.client, &state.config, &run, &messages).await;
    append_timeline(&mut run, "CORALOS_CONSOLE", console.note.clone());
    emit_trace(
        &app,
        &mut trace,
        &run.run_id,
        round + 1,
        AgentTracePhase::ToolResult,
        console.note.clone(),
        Some(json!({ "coralConsole": console })),
    );

    persist_run(
        state,
        &run,
        &context.event,
        &proof_receipt,
        maybe_decision.as_ref(),
        &llm_response,
    )?;
    let _ = coralos::transcript::persist_run_artifacts(
        &state.replay_dir,
        &run.run_id,
        &messages,
        &trace,
        Some(&proof_receipt),
    );
    for item in &run.timeline {
        let _ = app.emit(
            event_bus::MARKET_ROUND,
            MarketRoundEvent {
                run_id: run.run_id.clone(),
                phase: item.label.clone(),
                detail: item.detail.clone(),
                at: item.at.clone(),
            },
        );
    }
    let _ = app.emit(
        event_bus::APP_NOTIFICATION,
        json!({
            "title": "Match Intelligence Agent complete",
            "body": format!("{} produced {}", run.run_id, run.track),
            "ts": now_iso()
        }),
    );
    Ok(run)
}

async fn explain_decision(
    client: &reqwest::Client,
    config: &crate::config::AppConfig,
    context: &context::AgentContext,
    derived: &features::MarketFeatures,
) -> llm::LlmResponse {
    let request = llm::LlmRequest {
        system: [
            "You explain a Rust sports-data agent decision.",
            "Use only the supplied facts.",
            "Do not claim proof passed unless txoraclePassed is true.",
            "Do not recommend signing, payment release, or settlement.",
            "Return two concise sentences.",
        ]
        .join(" "),
        user: json!({
            "fixtureId": context.event.fixture_id,
            "track": context.track,
            "eventKind": format!("{:?}", context.event.kind),
            "title": &context.event.title,
            "features": derived
        })
        .to_string(),
        model: config.llm_model.clone(),
        max_tokens: 300,
        temperature: 0.2,
    };

    match llm::VeniceClient::new(client.clone())
        .complete(config, request)
        .await
    {
        Ok(response) => response,
        Err(err) => llm::LlmResponse::fallback(
            format!(
                "Deterministic explanation used: {}",
                feature_summary(derived)
            ),
            format!("llm_error:{err}"),
        ),
    }
}

fn empty_run(context: &context::AgentContext) -> AgentRun {
    AgentRun {
        run_id: context.run_id.clone(),
        track: context.track,
        trigger: context.event.clone(),
        bids: Vec::new(),
        winner: None,
        delivery: None,
        verdict: None,
        settlement: Some(SettlementReceipt {
            rail: None,
            status: SettlementStatus::NotStarted,
            reference: None,
            escrow_pda: None,
            deposit_tx: None,
            release_tx: None,
            explorer_url: None,
            chain_observed: Some(false),
            chain_slot: None,
            payment_url: None,
            payment_reference: None,
            payment_memo: None,
            payment_signature: None,
            payment_status: None,
            payment_recipient: None,
            payment_amount_sol: None,
        }),
        timeline: vec![TimelineEntry {
            at: now_iso(),
            label: "TRIGGER".to_string(),
            detail: format!("{:?}: {}", &context.event.kind, &context.event.title),
        }],
    }
}
