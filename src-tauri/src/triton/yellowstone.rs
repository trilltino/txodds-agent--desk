use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

use crate::config::AppConfig;
use crate::types::{now_iso, ChainStatus, Cluster};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
enum YellowstoneCommand {
    WatchAccount { account: String },
    WatchProgram { program_id: String },
    WatchReference { reference: String },
}

#[derive(Clone)]
pub struct YellowstoneHandle {
    tx: mpsc::UnboundedSender<YellowstoneCommand>,
}

impl YellowstoneHandle {
    pub fn watch_account(&self, account: String) {
        let _ = self.tx.send(YellowstoneCommand::WatchAccount { account });
    }

    pub fn watch_program(&self, program_id: String) {
        let _ = self
            .tx
            .send(YellowstoneCommand::WatchProgram { program_id });
    }

    pub fn watch_reference(&self, reference: String) {
        let _ = self
            .tx
            .send(YellowstoneCommand::WatchReference { reference });
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StreamStatus {
    source: String,
    state: String,
    detail: String,
}

pub fn spawn(app: AppHandle, config: AppConfig, sidecar_path: PathBuf) -> YellowstoneHandle {
    let (tx, rx) = mpsc::unbounded_channel();
    let handle = YellowstoneHandle { tx };
    tauri::async_runtime::spawn(run_sidecar(app, config, sidecar_path, rx));
    handle
}

async fn run_sidecar(
    app: AppHandle,
    config: AppConfig,
    sidecar_path: PathBuf,
    mut rx: mpsc::UnboundedReceiver<YellowstoneCommand>,
) {
    if !sidecar_path.exists() {
        emit_status(
            &app,
            "stopped",
            &format!(
                "Yellowstone sidecar not found at {}",
                sidecar_path.display()
            ),
        );
        return;
    }
    let Some(endpoint) = config.triton_grpc_endpoint.clone() else {
        emit_status(&app, "stopped", "TRITON_GRPC_ENDPOINT missing");
        return;
    };
    let Some(token) = config.triton_x_token.clone() else {
        emit_status(&app, "stopped", "TRITON_X_TOKEN missing");
        return;
    };

    let node = resolve_node_bin(&sidecar_path);
    let mut child = match Command::new(node)
        .arg(sidecar_path)
        .env("TRITON_GRPC_ENDPOINT", endpoint)
        .env("TRITON_X_TOKEN", token)
        .env(
            "WATCH_ESCROW_PROGRAM_ID",
            config.watch_escrow_program_id.clone().unwrap_or_default(),
        )
        .env(
            "WATCH_MARKET_PROGRAM_ID",
            config.watch_market_program_id.clone().unwrap_or_default(),
        )
        .env(
            "WATCH_ESCROW_ACCOUNT",
            config.watch_escrow_account.clone().unwrap_or_default(),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            emit_status(&app, "stopped", &format!("failed to spawn sidecar: {err}"));
            return;
        }
    };

    emit_status(&app, "connecting", "Yellowstone sidecar spawned");
    let Some(stdout) = child.stdout.take() else {
        emit_status(&app, "stopped", "failed to open Yellowstone stdout");
        return;
    };
    let Some(mut stdin) = child.stdin.take() else {
        emit_status(&app, "stopped", "failed to open Yellowstone stdin");
        return;
    };

    let writer = tauri::async_runtime::spawn(async move {
        while let Some(command) = rx.recv().await {
            if let Ok(line) = serde_json::to_string(&command) {
                if stdin.write_all(line.as_bytes()).await.is_err() {
                    break;
                }
                if stdin.write_all(b"\n").await.is_err() {
                    break;
                }
            }
        }
    });

    let mut lines = BufReader::new(stdout).lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => handle_sidecar_line(&app, &line),
            Ok(None) => break,
            Err(err) => {
                emit_status(
                    &app,
                    "reconnecting",
                    &format!("Yellowstone sidecar read failed: {err}"),
                );
                break;
            }
        }
    }
    writer.abort();
    let _ = child.wait().await;
    emit_status(&app, "stopped", "Yellowstone sidecar exited");
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

fn handle_sidecar_line(app: &AppHandle, line: &str) {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        emit_status(app, "reconnecting", line);
        return;
    };
    match value.get("event").and_then(Value::as_str) {
        Some("status") => emit_status(
            app,
            value
                .get("state")
                .and_then(Value::as_str)
                .unwrap_or("connected"),
            value
                .get("detail")
                .and_then(Value::as_str)
                .unwrap_or("Yellowstone update"),
        ),
        Some("slot") => {
            if let Some(slot) = value.get("slot").and_then(Value::as_u64) {
                let _ = app.emit(
                    "chain://slot",
                    ChainStatus {
                        cluster: Cluster::Devnet,
                        slot,
                        solana_core: "yellowstone".to_string(),
                        latency_ms: 0,
                        ts: now_iso(),
                    },
                );
            }
        }
        Some("account") => {
            let _ = app.emit(
                "chain://account",
                value.get("payload").cloned().unwrap_or(value),
            );
        }
        Some("tx") => {
            let _ = app.emit("chain://tx", value.get("payload").cloned().unwrap_or(value));
        }
        _ => {}
    }
}

fn emit_status(app: &AppHandle, state: &str, detail: &str) {
    let _ = app.emit(
        "ingest://status",
        StreamStatus {
            source: "yellowstone".to_string(),
            state: state.to_string(),
            detail: detail.to_string(),
        },
    );
}
