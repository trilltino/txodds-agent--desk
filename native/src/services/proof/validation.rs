//! Read-only txoracle validation sidecar integration.
//!
//! Rust owns TxLINE authentication, IDL path selection, and the no-fake-pass
//! policy. The Node sidecar only adapts the official Anchor IDL payload into a
//! `.view()` call, so validation never signs or submits a transaction.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

use crate::config::AppConfig;
use crate::error::AppError;
use crate::services::txline;
use crate::types::{AgentRun, Cluster, TxLineProofReceipt, ValidationSimulationStatus};

#[derive(Debug, Clone)]
pub struct ValidationBridge {
    sidecar_path: PathBuf,
    idl_dir: PathBuf,
}

impl ValidationBridge {
    pub fn new(sidecar_path: PathBuf, idl_dir: PathBuf) -> Self {
        Self {
            sidecar_path,
            idl_dir,
        }
    }

    pub async fn receipt_for_run(
        &self,
        client: &Client,
        config: &AppConfig,
        run: &AgentRun,
    ) -> TxLineProofReceipt {
        let mut receipt = super::receipt_for_run(run);
        receipt.txline_program = Some(config.txline_program_id());

        let cluster = txoracle_cluster(config);
        let idl_path = self.idl_path(cluster);
        if !idl_path.exists() {
            receipt.note = format!(
                "txoracle validation not started; official IDL missing at {}",
                idl_path.display()
            );
            receipt.raw = Some(json!({
                "source": "txoracle-validation",
                "status": "not_started",
                "reason": "missing_idl",
                "idlPath": idl_path.to_string_lossy()
            }));
            return receipt;
        }

        let proof_source = match load_proof_source(client, config, run).await {
            ProofLoad::Ready(source) => source,
            ProofLoad::NotStarted(note, raw) => {
                receipt.note = note;
                receipt.raw = Some(raw);
                return receipt;
            }
            ProofLoad::Unavailable(note, raw) => {
                receipt.simulation_status = ValidationSimulationStatus::Unavailable;
                receipt.note = note;
                receipt.raw = Some(raw);
                return receipt;
            }
        };

        receipt.seq = Some(proof_source.seq);
        receipt.stat_key = proof_source
            .stat_keys
            .first()
            .and_then(|value| value.parse::<u64>().ok());
        receipt.stat_keys = proof_source.stat_keys.clone();
        receipt.proof_present = true;

        let (rpc_url, rpc_headers) = rpc_config(config, cluster);
        let request = SidecarRequest {
            cmd: "simulateValidateStat".to_string(),
            payload: json!({
                "cluster": cluster_name(cluster),
                "programId": config.txline_program_id(),
                "fixtureId": run.trigger.fixture_id,
                "seq": proof_source.seq,
                "statKeys": proof_source.stat_keys.clone(),
                "txlineTs": run.trigger.txline_ts.clone(),
                "rpcUrl": rpc_url,
                "rpcHeaders": rpc_headers,
                "idlPath": idl_path.to_string_lossy(),
                "proof": proof_source.payload.clone()
            }),
        };

        match invoke_sidecar(&self.sidecar_path, &request).await {
            Ok(response) => merge_sidecar_response(receipt, proof_source.payload.clone(), response),
            Err(err) => {
                receipt.simulation_status = ValidationSimulationStatus::Unavailable;
                receipt.verified = false;
                receipt.note = format!("txoracle validation sidecar unavailable: {err}");
                receipt.raw = Some(json!({
                    "source": "txoracle-validation",
                    "status": "unavailable",
                    "reason": err.to_string()
                }));
                receipt
            }
        }
    }

    fn idl_path(&self, cluster: Cluster) -> PathBuf {
        self.idl_dir.join(match cluster {
            Cluster::Devnet => "txoracle.devnet.json",
            Cluster::Mainnet => "txoracle.mainnet.json",
        })
    }
}

#[derive(Debug)]
struct ProofSource {
    seq: u64,
    stat_keys: Vec<String>,
    payload: Value,
}

enum ProofLoad {
    Ready(ProofSource),
    NotStarted(String, Value),
    Unavailable(String, Value),
}

async fn load_proof_source(client: &Client, config: &AppConfig, run: &AgentRun) -> ProofLoad {
    if let Some(proof) = run
        .trigger
        .proof
        .as_ref()
        .and_then(|receipt| receipt.raw.clone())
        .filter(has_validation_shape)
    {
        if let Some(seq) = run.trigger.seq {
            return ProofLoad::Ready(ProofSource {
                seq,
                stat_keys: preferred_stat_keys(run),
                payload: proof,
            });
        }
        return ProofLoad::NotStarted(
            "txoracle validation not started; attached proof lacks TxLINE sequence".to_string(),
            json!({
                "source": "txoracle-validation",
                "status": "not_started",
                "reason": "missing_seq"
            }),
        );
    }

    let seq = match run.trigger.seq {
        Some(seq) => seq,
        None => {
            return ProofLoad::NotStarted(
                "txoracle validation not started; TxLINE event did not include seq".to_string(),
                json!({
                    "source": "txoracle-validation",
                    "status": "not_started",
                    "reason": "missing_seq",
                    "triggerEventId": &run.trigger.id
                }),
            );
        }
    };

    let stat_keys = numeric_stat_keys(run);
    if stat_keys.is_empty() {
        return ProofLoad::NotStarted(
            "txoracle validation not started; event has no numeric TxLINE statKeys".to_string(),
            json!({
                "source": "txoracle-validation",
                "status": "not_started",
                "reason": "missing_numeric_stat_keys",
                "triggerEventId": &run.trigger.id,
                "statKeys": &run.trigger.stat_keys
            }),
        );
    }

    let stat_keys_csv = stat_keys.join(",");
    let query = vec![
        ("fixtureId", run.trigger.fixture_id.to_string()),
        ("seq", seq.to_string()),
        ("statKeys", stat_keys_csv),
    ];
    match txline::api::authenticated_get(client, config, "api/scores/stat-validation", query).await
    {
        Ok(payload) => ProofLoad::Ready(ProofSource {
            seq,
            stat_keys,
            payload,
        }),
        Err(AppError::Config(err)) => ProofLoad::NotStarted(
            format!("txoracle validation not started; {err}"),
            json!({
                "source": "txoracle-validation",
                "status": "not_started",
                "reason": "missing_txline_credentials"
            }),
        ),
        Err(err) => ProofLoad::Unavailable(
            format!("txoracle validation proof fetch unavailable: {err}"),
            json!({
                "source": "txoracle-validation",
                "status": "unavailable",
                "reason": err.to_string()
            }),
        ),
    }
}

fn merge_sidecar_response(
    mut receipt: TxLineProofReceipt,
    proof_payload: Value,
    response: SidecarResponse,
) -> TxLineProofReceipt {
    receipt.simulation_status = response.status;
    receipt.verified = response.verified.unwrap_or(false)
        && response.proof_present.unwrap_or(false)
        && response.root_present.unwrap_or(false)
        && matches!(
            receipt.simulation_status,
            ValidationSimulationStatus::Passed
        );
    receipt.proof_present = response.proof_present.unwrap_or(receipt.proof_present);
    receipt.root_present = response.root_present.unwrap_or(false);
    receipt.root_pda = response.root_pda;
    receipt.txline_program = response
        .program_id
        .or_else(|| receipt.txline_program.clone());
    receipt.root_observed_slot = response.root_observed_slot;
    receipt.epoch_day = response.epoch_day.or(receipt.epoch_day);
    receipt.txline_ts = response.txline_ts.or_else(|| receipt.txline_ts.clone());
    receipt.merkle_root = response.merkle_root;
    receipt.stat_proof_hash = response.stat_proof_hash;
    receipt.note = response.reason.unwrap_or_else(|| {
        if receipt.verified {
            "txoracle validation sidecar passed".to_string()
        } else {
            "txoracle validation sidecar did not pass".to_string()
        }
    });
    receipt.raw = Some(json!({
        "source": "txoracle-validation-sidecar",
        "method": response.method,
        "sidecar": response.raw,
        "txlineProof": proof_payload
    }));
    receipt
}

fn has_validation_shape(value: &Value) -> bool {
    let inner = value.get("validation").unwrap_or(value);
    inner.get("summary").is_some()
        && (inner.get("statsToProve").is_some() || inner.get("statToProve").is_some())
}

fn preferred_stat_keys(run: &AgentRun) -> Vec<String> {
    if !run.trigger.stat_keys.is_empty() {
        return run.trigger.stat_keys.clone();
    }
    run.trigger
        .proof
        .as_ref()
        .map(|proof| proof.stat_keys.clone())
        .unwrap_or_default()
}

fn numeric_stat_keys(run: &AgentRun) -> Vec<String> {
    let mut keys = Vec::new();
    let preferred = preferred_stat_keys(run);
    for value in run.trigger.stat_keys.iter().chain(preferred.iter()) {
        push_numeric_key(&mut keys, value);
    }
    if let Some(key) = run.trigger.proof.as_ref().and_then(|proof| proof.stat_key) {
        push_numeric_key(&mut keys, &key.to_string());
    }
    collect_numeric_keys_from_json(&mut keys, run.trigger.raw.as_ref());
    keys.sort();
    keys.dedup();
    keys
}

fn collect_numeric_keys_from_json(keys: &mut Vec<String>, raw: Option<&Value>) {
    let Some(raw) = raw else {
        return;
    };
    for field in [
        "statKey",
        "stat_key",
        "StatKey",
        "statKeys",
        "stat_keys",
        "StatKeys",
    ] {
        match raw.get(field) {
            Some(Value::Number(number)) => push_numeric_key(keys, &number.to_string()),
            Some(Value::String(value)) => {
                for item in value.split(',') {
                    push_numeric_key(keys, item);
                }
            }
            Some(Value::Array(values)) => {
                for item in values {
                    match item {
                        Value::Number(number) => push_numeric_key(keys, &number.to_string()),
                        Value::String(value) => push_numeric_key(keys, value),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
}

fn push_numeric_key(keys: &mut Vec<String>, value: &str) {
    let trimmed = value.trim();
    if !trimmed.is_empty() && trimmed.parse::<u64>().is_ok() {
        keys.push(trimmed.to_string());
    }
}

fn txoracle_cluster(config: &AppConfig) -> Cluster {
    if config.txline_network.eq_ignore_ascii_case("mainnet")
        || config.solana_cluster.eq_ignore_ascii_case("mainnet")
    {
        Cluster::Mainnet
    } else {
        Cluster::Devnet
    }
}

fn rpc_config(config: &AppConfig, cluster: Cluster) -> (String, BTreeMap<String, String>) {
    let mut headers = BTreeMap::new();
    if let Some(token) = config.rpc_token(cluster) {
        headers.insert("x-token".to_string(), token.to_string());
    }
    let url = config
        .rpc_endpoint(cluster)
        .map(str::to_string)
        .unwrap_or_else(|| match cluster {
            Cluster::Devnet => "https://api.devnet.solana.com".to_string(),
            Cluster::Mainnet => "https://api.mainnet-beta.solana.com".to_string(),
        });
    (url, headers)
}

fn cluster_name(cluster: Cluster) -> &'static str {
    match cluster {
        Cluster::Devnet => "devnet",
        Cluster::Mainnet => "mainnet",
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SidecarRequest {
    cmd: String,
    payload: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SidecarResponse {
    #[allow(dead_code)]
    ok: bool,
    status: ValidationSimulationStatus,
    verified: Option<bool>,
    reason: Option<String>,
    method: Option<String>,
    program_id: Option<String>,
    root_pda: Option<String>,
    root_present: Option<bool>,
    root_observed_slot: Option<u64>,
    proof_present: Option<bool>,
    epoch_day: Option<u32>,
    txline_ts: Option<String>,
    merkle_root: Option<String>,
    stat_proof_hash: Option<String>,
    raw: Option<Value>,
}

async fn invoke_sidecar(
    sidecar_path: &Path,
    request: &SidecarRequest,
) -> Result<SidecarResponse, AppError> {
    if !sidecar_path.exists() {
        return Err(AppError::Config(format!(
            "txoracle validation sidecar not found at {}",
            sidecar_path.display()
        )));
    }

    let node = resolve_node_bin(sidecar_path);
    let mut child = Command::new(node)
        .arg(sidecar_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| AppError::Task("failed to open txoracle sidecar stdin".to_string()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::Task("failed to open txoracle sidecar stdout".to_string()))?;

    let line = serde_json::to_string(request)?;
    stdin.write_all(line.as_bytes()).await?;
    stdin.write_all(b"\n").await?;
    drop(stdin);

    let mut reader = BufReader::new(stdout).lines();
    let response_line =
        tokio::time::timeout(std::time::Duration::from_secs(45), reader.next_line())
            .await
            .map_err(|_| AppError::Task("txoracle validation sidecar timed out".to_string()))??
            .ok_or_else(|| {
                AppError::Task("txoracle validation sidecar exited without a response".to_string())
            })?;

    let _ = child.wait().await;
    Ok(serde_json::from_str::<SidecarResponse>(&response_line)?)
}

fn resolve_node_bin(sidecar_path: &Path) -> PathBuf {
    if let Ok(path) = std::env::var("NODE_BIN") {
        return PathBuf::from(path);
    }

    if let Some(sidecar_dir) = sidecar_path.parent() {
        let bundled = sidecar_dir.join("bin").join("node.exe");
        if bundled.exists() {
            return bundled;
        }
        if let Some(resource_dir) = sidecar_dir.parent() {
            let bundled = resource_dir.join("bin").join("node.exe");
            if bundled.exists() {
                return bundled;
            }
        }
    }

    PathBuf::from("node")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{now_iso, TxLineEvent, TxLineEventKind};

    #[test]
    fn only_numeric_stat_keys_are_used_for_validation_fetches() {
        let mut event = test_event();
        event.stat_keys = vec![
            "score.home".to_string(),
            "1002".to_string(),
            "1003".to_string(),
        ];
        event.raw = Some(json!({ "statKeys": "1003,1004", "statKey": 1005 }));
        let run = AgentRun {
            run_id: "run".to_string(),
            track: crate::types::TrackMode::Settlement,
            trigger: event,
            bids: vec![],
            winner: None,
            delivery: None,
            verdict: None,
            settlement: None,
            timeline: vec![],
        };

        assert_eq!(
            numeric_stat_keys(&run),
            vec!["1002", "1003", "1004", "1005"]
        );
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
            stat_keys: vec![],
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
