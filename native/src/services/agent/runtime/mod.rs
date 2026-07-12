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
mod fan_pundit_delegation;
mod handoff;
mod persistence;
mod settlement_delegation;
mod signal;
mod trading_delegation;
mod wager_proof_delegation;

use decision::{action_summary, apply_decision_to_run};
use events::{append_timeline, emit_message, emit_trace, proof_event};
use handoff::{specialist_for_track, wager_ruling_payload};
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
    // Real CoralOS session/thread, created on first use by the wager-proof
    // delegation below and reused by `publish_run`'s end-of-round batch —
    // see `wager_proof_delegation` module docs for why this can't wait
    // until the end of the round the way the rest of the transcript does.
    //
    // Eagerly reuse the app-lifetime session (`state.coralos_session_id`)
    // here, before any delegation runs, so every delegation call below takes
    // its `Some(session) => reuse` branch instead of minting a fresh session.
    // Without this, every single round created its own session plus four
    // fresh Docker containers that then sat idle and wound down within
    // ~60s — invisible by the time anyone checked the Console. A stale
    // persisted id (e.g. after a coral-server restart) self-heals: on
    // failure the round falls back to its normal per-delegation lazy
    // creation, and the next round's persisted id is refreshed below.
    let mut live_session: Option<coralos::console::LiveSession> = None;
    if state.config.coralos_console_enabled {
        let persisted_session_id = state
            .coralos_session_id
            .lock()
            .ok()
            .and_then(|guard| guard.clone());
        match coralos::console::ensure_session_with_override(
            &state.client,
            &state.config,
            &run,
            persisted_session_id,
        )
        .await
        {
            Ok(session) => {
                if let Ok(mut guard) = state.coralos_session_id.lock() {
                    *guard = Some(session.session_id.clone());
                }
                live_session = Some(session);
            }
            Err(_) => {
                // Clear a possibly-stale id so the next round doesn't retry it.
                if let Ok(mut guard) = state.coralos_session_id.lock() {
                    *guard = None;
                }
            }
        }
    }
    // The proof-guard-verified wager, once the wager debate below produces
    // one — the Settlement/Fan track specialist delegations further down
    // use this (the real, verified wager), not the raw pre-proof one.
    let mut verified_wager: Option<txodds_types::wager::Wager> = None;
    let mut round = 1_u64;

    let _ = app.emit(event_bus::CORAL_SESSION, &session);
    // Observing/normalizing the live TxLINE event and deriving features are
    // match-intelligence-agent's own internal steps, not separate actors —
    // narrated as itself (to user-proxy, the human), not as three fake
    // personas the way this used to attribute them to
    // "txline-ingest"/"txline-normalizer"/"feature-extractor".
    emit_message(
        &app,
        &session,
        &mut messages,
        round,
        MATCH_INTELLIGENCE_AGENT,
        vec![USER_PROXY],
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
        MATCH_INTELLIGENCE_AGENT,
        vec![USER_PROXY],
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
    // Deriving features is match-intelligence-agent's own deterministic
    // step (agent_core::features), not a call to a separate agent — a
    // single self-narrated ToolResult, not a fake TOOL_CALL/TOOL_RESULT
    // round-trip to a "feature-extractor" persona that never existed.
    emit_message(
        &app,
        &session,
        &mut messages,
        round,
        MATCH_INTELLIGENCE_AGENT,
        vec![USER_PROXY],
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
    // Accumulated reasoning trail for this round (TODO 6e): every Venice
    // tool call the wager debate actually made, carried on the delegation
    // wire messages below so the Coral transcript shows *why*, not just the
    // verdict.
    let mut reasoning_trail = wager_outcome.tool_trail.clone();
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
        Some(wager_ruling_payload(wager_outcome.ruling.as_ref(), &wager_outcome.tool_trail)),
    );
    emit_trace(
        &app,
        &mut trace,
        &run.run_id,
        round,
        AgentTracePhase::ToolResult,
        wager_outcome.narrative.clone(),
        Some(wager_ruling_payload(wager_outcome.ruling.as_ref(), &wager_outcome.tool_trail)),
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
        reasoning_trail.extend(pundit_outcome.tool_trail.iter().cloned());
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
            Some(wager_ruling_payload(
                pundit_outcome.updated_ruling.as_ref(),
                &pundit_outcome.tool_trail,
            )),
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

        // ── Real wager-consistency proof gate (rig-venice ROADMAP.md Phase 6,
        //    step 1) ───────────────────────────────────────────────────────
        //
        // Unlike everything above, this is not a Rust function call narrated
        // as a Coral message: `proof-guard-agent` is a separate OS process
        // that coral-server launches and that blocks on its own
        // `wait_for_mention` loop. This orchestrator sends the delegation
        // over a real CoralOS thread and polls for the real reply — see
        // `wager_proof_delegation` for why that requires the session/thread
        // to exist now, not at the end of the round like the rest of this
        // transcript.
        let final_wager = pundit_outcome
            .updated_ruling
            .as_ref()
            .unwrap_or(ruling)
            .wager
            .clone();
        round += 1;
        emit_message(
            &app,
            &session,
            &mut messages,
            round,
            MATCH_INTELLIGENCE_AGENT,
            vec![PROOF_GUARD_AGENT],
            CoralVerb::ToolCall,
            format!(
                "DELEGATE wager {} to proof-guard-agent for consistency verification",
                final_wager.wager_id
            ),
            Some(json!({ "wagerId": &final_wager.wager_id })),
        );
        let real_verdict = wager_proof_delegation::delegate(
            &state.client,
            &state.config,
            &run,
            &mut live_session,
            &final_wager,
            &reasoning_trail,
        )
        .await;
        round += 1;
        emit_message(
            &app,
            &session,
            &mut messages,
            round,
            PROOF_GUARD_AGENT,
            vec![MATCH_INTELLIGENCE_AGENT, USER_PROXY],
            CoralVerb::ToolResult,
            real_verdict.reason.clone(),
            Some(json!({ "wager": &real_verdict.wager })),
        );
        emit_trace(
            &app,
            &mut trace,
            &run.run_id,
            round,
            AgentTracePhase::Proof,
            real_verdict.reason.clone(),
            Some(json!({ "wager": &real_verdict.wager })),
        );
        append_timeline(
            &mut run,
            "WAGER_PROOF_GUARD",
            format!("{:?}: {}", real_verdict.wager.status, real_verdict.reason),
        );
        verified_wager = Some(real_verdict.wager);
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
        // --- multi-agent handoff: delegate to the real track specialist ---
        //
        // Unlike the old `specialist_ack()` stub, this is a real delegation:
        // each branch below sends an actual message to a genuinely
        // independent OS process (settlement-agent / sharp-movement-detector
        // (trading-specialist) / fan-pundit-agent) and waits for its real
        // reply, mirroring `wager_proof_delegation`. The `DELEGATE ...`
        // message below stays local-only narration for the retrospective
        // Console transcript; the real wire message each `*_delegation`
        // module sends has its own specialist-specific grammar.
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

        let (handoff_status, handoff_reason) = match context.track {
            TrackMode::Settlement => match &verified_wager {
                Some(wager) => {
                    let outcome = settlement_delegation::delegate(
                        &state.client,
                        &state.config,
                        &run,
                        &mut live_session,
                        wager,
                        &reasoning_trail,
                    )
                    .await;
                    let status = outcome.status.clone();
                    let reason = outcome.reason.clone();
                    emit_message(
                        &app,
                        &session,
                        &mut messages,
                        round,
                        specialist,
                        vec![MATCH_INTELLIGENCE_AGENT, USER_PROXY],
                        CoralVerb::ToolResult,
                        reason.clone(),
                        Some(json!({ "status": &outcome.status, "txSig": &outcome.tx_sig })),
                    );
                    (status, reason)
                }
                None => {
                    let reason = "no verified wager to settle this round".to_string();
                    emit_message(
                        &app,
                        &session,
                        &mut messages,
                        round,
                        MATCH_INTELLIGENCE_AGENT,
                        vec![USER_PROXY],
                        CoralVerb::ToolResult,
                        reason.clone(),
                        None,
                    );
                    ("skipped".to_string(), reason)
                }
            },
            TrackMode::Trading => {
                let outcome = trading_delegation::delegate(
                    &state.client,
                    &state.config,
                    &run,
                    &mut live_session,
                    &signal,
                    &decision,
                )
                .await;
                let status = outcome.status.clone();
                let reason = outcome.reason.clone();
                emit_message(
                    &app,
                    &session,
                    &mut messages,
                    round,
                    specialist,
                    vec![MATCH_INTELLIGENCE_AGENT, USER_PROXY],
                    CoralVerb::ToolResult,
                    reason.clone(),
                    Some(json!({
                        "status": &outcome.status,
                        "positionId": &outcome.position_id,
                        "sizeSol": outcome.size_sol
                    })),
                );
                (status, reason)
            }
            TrackMode::Fan => match &verified_wager {
                Some(wager) => {
                    let outcome = fan_pundit_delegation::delegate(
                        &state.client,
                        &state.config,
                        &run,
                        &mut live_session,
                        wager,
                        &reasoning_trail,
                    )
                    .await;
                    let status = outcome.stance.clone();
                    let reason = outcome.reason.clone();
                    emit_message(
                        &app,
                        &session,
                        &mut messages,
                        round,
                        specialist,
                        vec![MATCH_INTELLIGENCE_AGENT, USER_PROXY],
                        CoralVerb::ToolResult,
                        reason.clone(),
                        Some(json!({ "stance": &outcome.stance, "wager": &outcome.wager })),
                    );
                    (status, reason)
                }
                None => {
                    let reason = "no verified wager for fan-pundit-agent to react to".to_string();
                    emit_message(
                        &app,
                        &session,
                        &mut messages,
                        round,
                        MATCH_INTELLIGENCE_AGENT,
                        vec![USER_PROXY],
                        CoralVerb::ToolResult,
                        reason.clone(),
                        None,
                    );
                    ("skipped".to_string(), reason)
                }
            },
        };
        emit_trace(
            &app,
            &mut trace,
            &run.run_id,
            round,
            AgentTracePhase::Action,
            format!("specialist {specialist} real verdict: {handoff_status}"),
            Some(json!({ "specialist": specialist, "status": &handoff_status })),
        );
        append_timeline(
            &mut run,
            "HANDOFF",
            format!("{MATCH_INTELLIGENCE_AGENT} → {specialist}: {handoff_status} — {handoff_reason}"),
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

    let console = coralos::console::publish_run(
        &state.client,
        &state.config,
        &run,
        &messages,
        live_session,
    )
    .await;
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
