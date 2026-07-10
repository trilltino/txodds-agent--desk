//! Ledger persistence for a completed Match Intelligence Agent round.

use crate::domain::agent::AgentDecision;
use crate::error::AppError;
use crate::services::llm;
use crate::state::DesktopState;
use crate::types::{AgentRun, TxLineEvent, TxLineProofReceipt};

pub(super) fn persist_run(
    state: &DesktopState,
    run: &AgentRun,
    event: &TxLineEvent,
    proof_receipt: &TxLineProofReceipt,
    decision: Option<&AgentDecision>,
    llm_response: &llm::LlmResponse,
) -> Result<(), AppError> {
    let ledger = state
        .ledger
        .lock()
        .map_err(|_| AppError::Task("ledger lock poisoned".to_string()))?;
    ledger.upsert_run(run)?;
    ledger.insert_agent_observation(&run.run_id, event)?;
    ledger.insert_proof_receipt(&run.run_id, proof_receipt)?;
    ledger.insert_llm_call(&run.run_id, llm_response)?;
    if let Some(decision) = decision {
        ledger.insert_agent_decision(&run.run_id, decision)?;
    }
    Ok(())
}
