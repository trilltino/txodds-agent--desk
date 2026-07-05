//! CoralOS settlement sidecar bridge.
//!
//! Rust owns the policy boundary and process supervision. The Node sidecar adapts
//! to existing CoralOS/TxODDS routes and returns a normalized receipt.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

use crate::config::AppConfig;
use crate::error::AppError;
use crate::types::{AgentRun, SettlementReceipt, SettlementStatus};

#[derive(Debug, Clone)]
pub struct SettlementBridge {
    // Resolved path to runtime/sidecars/coralos-bridge.mjs or an override.
    sidecar_path: PathBuf,
}

impl SettlementBridge {
    pub fn new(sidecar_path: PathBuf) -> Self {
        Self { sidecar_path }
    }

    // Attempt settlement for a verified run. The caller decides whether failure
    // is fatal; current demo flow records settlement failure but still persists
    // the run.
    pub async fn settle_run(
        &self,
        config: &AppConfig,
        run: &AgentRun,
    ) -> Result<SettlementReceipt, AppError> {
        if !config.coralos_settlement_enabled {
            // Feature gate prevents accidental settlement attempts on machines
            // that should only simulate.
            return Err(AppError::Config("CORALOS_SETTLEMENT_ENABLED=0".to_string()));
        }
        if !self.sidecar_path.exists() {
            return Err(AppError::Config(format!(
                "CoralOS sidecar not found at {}",
                self.sidecar_path.display()
            )));
        }

        // Use the market-generated settlement reference when available.
        let reference = run
            .settlement
            .as_ref()
            .and_then(|settlement| settlement.reference.clone());
        // Demo settlement amount follows the winning bid, with a conservative
        // fallback bounded by config.
        let amount_sol = run
            .winner
            .as_ref()
            .map(|winner| winner.price_sol)
            .unwrap_or(config.max_devnet_spend_sol.min(0.001).max(0.001));

        // Payload intentionally includes the full run context but no private
        // key material. Sidecar secrets come from env/config, not the webview.
        let request = SidecarRequest {
            cmd: "settleRun".to_string(),
            run_id: run.run_id.clone(),
            fixture_id: run.trigger.fixture_id.to_string(),
            amount_sol,
            reference,
            payload: json!({
                "track": run.track,
                "trigger": run.trigger,
                "delivery": run.delivery,
                "verdict": run.verdict,
                "coralos": {
                    "root": config.coralos_root,
                    "bridgeUrl": config.coralos_bridge_url,
                    "proxyUrl": config.coralos_proxy_url,
                    "serverUrl": config.coralos_server_url,
                    "token": config.coralos_token
                }
            }),
        };

        let response = invoke_sidecar(&self.sidecar_path, &request).await?;
        if !response.ok {
            return Err(AppError::Task(response.error.unwrap_or_else(|| {
                "CoralOS sidecar returned ok=false".to_string()
            })));
        }
        Ok(response.into_receipt(amount_sol))
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SidecarRequest {
    // Command name for versionable NDJSON sidecar IPC.
    cmd: String,
    run_id: String,
    fixture_id: String,
    amount_sol: f64,
    reference: Option<String>,
    payload: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SidecarResponse {
    ok: bool,
    error: Option<String>,
    reference: Option<String>,
    escrow_pda: Option<String>,
    deposit_sig: Option<String>,
    release_sig: Option<String>,
    explorer_url: Option<String>,
}

impl SidecarResponse {
    fn into_receipt(self, _amount_sol: f64) -> SettlementReceipt {
        // Infer lifecycle status from the strongest returned signature/PDA.
        let status = if self.release_sig.is_some() {
            SettlementStatus::Released
        } else if self.deposit_sig.is_some() {
            SettlementStatus::Deposited
        } else if self.escrow_pda.is_some() {
            SettlementStatus::EscrowCreated
        } else {
            SettlementStatus::NotStarted
        };
        SettlementReceipt {
            rail: Some("coralos".to_string()),
            status,
            reference: self.reference,
            escrow_pda: self.escrow_pda,
            deposit_tx: self.deposit_sig,
            release_tx: self.release_sig,
            explorer_url: self.explorer_url,
            triton_observed: Some(false),
            triton_slot: None,
            payment_url: None,
            payment_reference: None,
            payment_memo: None,
            payment_signature: None,
            payment_status: None,
            payment_recipient: None,
            payment_amount_sol: None,
        }
    }
}

async fn invoke_sidecar(
    sidecar_path: &Path,
    request: &SidecarRequest,
) -> Result<SidecarResponse, AppError> {
    let node = resolve_node_bin(sidecar_path);
    // Spawn a short-lived sidecar invocation. Stdin/stdout are NDJSON; stderr is
    // reserved for diagnostics from the child process.
    let mut child = Command::new(node)
        .arg(sidecar_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| AppError::Task("failed to open sidecar stdin".to_string()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::Task("failed to open sidecar stdout".to_string()))?;

    // Send exactly one request line and close stdin so the sidecar can complete
    // and return exactly one response line.
    let line = serde_json::to_string(request)?;
    stdin.write_all(line.as_bytes()).await?;
    stdin.write_all(b"\n").await?;
    drop(stdin);

    let mut reader = BufReader::new(stdout).lines();
    // Settlement can touch external devnet/proxy infrastructure, so allow a
    // longer timeout than ordinary HTTP RPC.
    let response_line =
        tokio::time::timeout(std::time::Duration::from_secs(90), reader.next_line())
            .await
            .map_err(|_| AppError::Task("CoralOS sidecar timed out".to_string()))??
            .ok_or_else(|| {
                AppError::Task("CoralOS sidecar exited without a response".to_string())
            })?;

    let _ = child.wait().await;
    Ok(serde_json::from_str::<SidecarResponse>(&response_line)?)
}

fn resolve_node_bin(sidecar_path: &Path) -> PathBuf {
    // NODE_BIN is useful during development or if packaging changes.
    if let Ok(path) = std::env::var("NODE_BIN") {
        return PathBuf::from(path);
    }

    if let Some(sidecar_dir) = sidecar_path.parent() {
        // Current bundled layout: runtime/sidecars/bin/node.exe.
        let bundled = sidecar_dir.join("bin").join("node.exe");
        if bundled.exists() {
            return bundled;
        }
        if let Some(resource_dir) = sidecar_dir.parent() {
            // Alternate packaged layout fallback.
            let bundled = resource_dir.join("bin").join("node.exe");
            if bundled.exists() {
                return bundled;
            }
        }
    }

    PathBuf::from("node")
}
