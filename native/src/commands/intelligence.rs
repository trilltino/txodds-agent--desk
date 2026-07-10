//! Intelligence-track commands: real Match Intelligence Agent execution.
//!
//! `run_agent_round` is now a thin IPC adapter over
//! `services::agent::runtime::run_match_intelligence_round`. It no longer calls
//! the legacy Coral market simulator.

use tauri::{AppHandle, State};

use crate::error::AppError;
use crate::services::agent;
use crate::services::coral;
use crate::state::DesktopState;
use crate::types::{
    AgentRun, SettlementReceipt, TimelineEntry, TrackMode, TxLineEvent, VerdictStatus,
};

#[tauri::command]
pub async fn run_agent_round(
    trigger: TxLineEvent,
    track: TrackMode,
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<AgentRun, AppError> {
    agent::runtime::run_match_intelligence_round(app, &state, trigger, track).await
}

#[tauri::command]
pub fn list_runs(state: State<'_, DesktopState>) -> Result<Vec<AgentRun>, AppError> {
    let ledger = state
        .ledger
        .lock()
        .map_err(|_| AppError::Task("ledger lock poisoned".to_string()))?;
    ledger.list_runs()
}

#[tauri::command]
pub fn get_run(run_id: String, state: State<'_, DesktopState>) -> Result<AgentRun, AppError> {
    let ledger = state
        .ledger
        .lock()
        .map_err(|_| AppError::Task("ledger lock poisoned".to_string()))?;
    ledger.get_run(&run_id)
}

#[tauri::command]
pub fn list_coral_agents() -> Vec<coral::agents::CoralAgentManifest> {
    coral::agents::built_in_agents()
}

/// Deterministic settlement gate: only txoracle-proof-passed settlement runs may
/// create payment intents or reach settlement. This is code-only by design.
pub(crate) fn verifier_passed(run: &AgentRun) -> bool {
    matches!(run.track, TrackMode::Settlement)
        && run
            .verdict
            .as_ref()
            .map(|verdict| matches!(verdict.status, VerdictStatus::Pass))
            .unwrap_or(false)
}

/// Merge an incoming settlement receipt into the run, keeping any established
/// payment-rail fields that the newer receipt does not override.
pub(crate) fn merge_receipt(run: &mut AgentRun, mut incoming: SettlementReceipt) {
    if let Some(existing) = run.settlement.take() {
        let keep_primary_payment_rail =
            existing.payment_url.is_some() && incoming.payment_url.is_none();
        if keep_primary_payment_rail || incoming.rail.is_none() {
            incoming.rail = existing.rail;
        }
        if incoming.reference.is_none() {
            incoming.reference = existing.reference;
        }
        if incoming.escrow_pda.is_none() {
            incoming.escrow_pda = existing.escrow_pda;
        }
        if incoming.deposit_tx.is_none() {
            incoming.deposit_tx = existing.deposit_tx;
        }
        if incoming.release_tx.is_none() {
            incoming.release_tx = existing.release_tx;
        }
        if incoming.explorer_url.is_none() {
            incoming.explorer_url = existing.explorer_url;
        }
        if incoming.chain_observed.is_none() {
            incoming.chain_observed = existing.chain_observed;
        }
        if incoming.chain_slot.is_none() {
            incoming.chain_slot = existing.chain_slot;
        }
        if incoming.payment_url.is_none() {
            incoming.payment_url = existing.payment_url;
        }
        if incoming.payment_reference.is_none() {
            incoming.payment_reference = existing.payment_reference;
        }
        if incoming.payment_memo.is_none() {
            incoming.payment_memo = existing.payment_memo;
        }
        if incoming.payment_signature.is_none() {
            incoming.payment_signature = existing.payment_signature;
        }
        if incoming.payment_status.is_none() {
            incoming.payment_status = existing.payment_status;
        }
        if incoming.payment_recipient.is_none() {
            incoming.payment_recipient = existing.payment_recipient;
        }
        if incoming.payment_amount_sol.is_none() {
            incoming.payment_amount_sol = existing.payment_amount_sol;
        }
    }
    run.settlement = Some(incoming);
}

pub(crate) fn append_timeline(
    run: &mut AgentRun,
    label: impl Into<String>,
    detail: impl Into<String>,
) {
    run.timeline.push(TimelineEntry {
        at: crate::types::now_iso(),
        label: label.into(),
        detail: detail.into(),
    });
}
