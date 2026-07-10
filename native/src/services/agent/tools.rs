//! Local tools available to the Match Intelligence Agent.

use crate::config::AppConfig;
use crate::services::proof::{self, ProofGateDecision, ValidationBridge};
use crate::types::{AgentRun, TxLineProofReceipt};

pub async fn request_proof(
    bridge: &ValidationBridge,
    http: &reqwest::Client,
    config: &AppConfig,
    run: &AgentRun,
) -> (TxLineProofReceipt, ProofGateDecision) {
    let receipt = bridge.receipt_for_run(http, config, run).await;
    let gate = proof::gate_receipt(run, &receipt);
    (receipt, gate)
}
