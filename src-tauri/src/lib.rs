mod config;
mod coral;
mod error;
mod ledger;
mod triton;
mod txline;
mod types;
mod web;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::config::{AppConfig, PublicConfig};
use crate::error::AppError;
use crate::ledger::LedgerStore;
use crate::types::{
    now_iso, AgentRun, ChainStatus, Cluster, MarketRoundEvent, TrackMode, TritonObservation,
    TxLineEvent,
};

struct DesktopState {
    config: AppConfig,
    client: Client,
    ledger: Arc<Mutex<LedgerStore>>,
    txline_task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    yellowstone: Option<triton::yellowstone::YellowstoneHandle>,
    settlement_bridge: coral::settlement::SettlementBridge,
    replay_dir: PathBuf,
    export_dir: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HashReceipt {
    sha256: String,
    reference: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportResult {
    path: String,
    share_text: String,
}

#[tauri::command]
fn get_config(state: State<'_, DesktopState>) -> PublicConfig {
    state.config.public()
}

#[tauri::command]
fn list_coral_agents() -> Vec<coral::agents::CoralAgentManifest> {
    coral::agents::built_in_agents()
}

#[tauri::command]
fn hash_delivery(payload: String) -> HashReceipt {
    let sha256 = sha256_hex(&payload);
    HashReceipt {
        reference: format!("sha256:{sha256}"),
        sha256,
    }
}

#[tauri::command]
async fn chain_rpc(
    cluster: Cluster,
    method: String,
    params: Option<Value>,
    state: State<'_, DesktopState>,
) -> Result<Value, AppError> {
    triton::rpc::triton_rpc(
        &state.client,
        &state.config,
        cluster,
        &method,
        params.unwrap_or(Value::Array(vec![])),
    )
    .await
}

#[tauri::command]
async fn chain_status(
    cluster: Cluster,
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<ChainStatus, AppError> {
    let status = triton::rpc::chain_status(&state.client, &state.config, cluster).await?;
    let _ = app.emit("chain://slot", &status);
    Ok(status)
}

#[tauri::command]
async fn observe_settlement(
    reference: String,
    escrow_account: Option<String>,
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<TritonObservation, AppError> {
    if let Some(yellowstone) = &state.yellowstone {
        yellowstone.watch_reference(reference.clone());
        if let Some(account) = escrow_account.clone() {
            yellowstone.watch_account(account);
        }
    }
    let observation =
        triton::rpc::observe_settlement(&state.client, &state.config, reference, escrow_account)
            .await?;
    let _ = app.emit("settle://receipt", &observation);
    Ok(observation)
}

#[tauri::command]
fn watch_account(account: String, state: State<'_, DesktopState>) -> Result<(), AppError> {
    let yellowstone = state
        .yellowstone
        .as_ref()
        .ok_or_else(|| AppError::Config("Yellowstone is not configured".to_string()))?;
    yellowstone.watch_account(account);
    Ok(())
}

#[tauri::command]
fn watch_program(program_id: String, state: State<'_, DesktopState>) -> Result<(), AppError> {
    let yellowstone = state
        .yellowstone
        .as_ref()
        .ok_or_else(|| AppError::Config("Yellowstone is not configured".to_string()))?;
    yellowstone.watch_program(program_id);
    Ok(())
}

#[tauri::command]
fn watch_reference(reference: String, state: State<'_, DesktopState>) -> Result<(), AppError> {
    let yellowstone = state
        .yellowstone
        .as_ref()
        .ok_or_else(|| AppError::Config("Yellowstone is not configured".to_string()))?;
    yellowstone.watch_reference(reference);
    Ok(())
}

#[tauri::command]
async fn start_txline(
    mode: String,
    fixture_id: Option<String>,
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<(), AppError> {
    {
        let mut task = state
            .txline_task
            .lock()
            .map_err(|_| AppError::Task("txline task lock poisoned".to_string()))?;
        if let Some(handle) = task.take() {
            handle.abort();
        }
    }
    let handle = txline::spawn_txline(
        app,
        state.client.clone(),
        state.config.clone(),
        mode,
        fixture_id,
        state.replay_dir.clone(),
    );
    let mut task = state
        .txline_task
        .lock()
        .map_err(|_| AppError::Task("txline task lock poisoned".to_string()))?;
    *task = Some(handle);
    Ok(())
}

#[tauri::command]
fn stop_txline(state: State<'_, DesktopState>) -> Result<(), AppError> {
    let mut task = state
        .txline_task
        .lock()
        .map_err(|_| AppError::Task("txline task lock poisoned".to_string()))?;
    if let Some(handle) = task.take() {
        handle.abort();
    }
    Ok(())
}

#[tauri::command]
async fn run_agent_round(
    trigger: TxLineEvent,
    track: TrackMode,
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<AgentRun, AppError> {
    let mut run = coral::market::run_round(trigger, track);

    if let Some(reference) = run
        .settlement
        .as_ref()
        .and_then(|settlement| settlement.reference.clone())
    {
        match state
            .settlement_bridge
            .settle_run(&state.config, &run)
            .await
        {
            Ok(receipt) => {
                let detail = format!(
                    "CoralOS sidecar settled reference {}",
                    receipt
                        .reference
                        .clone()
                        .unwrap_or_else(|| reference.clone())
                );
                run.settlement = Some(receipt.clone());
                coral::market::append_timeline(&mut run, "CORALOS", detail);
                let _ = app.emit("settle://receipt", &receipt);
                if let Some(yellowstone) = &state.yellowstone {
                    if let Some(account) = receipt.escrow_pda.clone() {
                        yellowstone.watch_account(account);
                    }
                    if let Some(reference) = receipt.reference.clone() {
                        yellowstone.watch_reference(reference);
                    }
                }
            }
            Err(err) => {
                coral::market::append_timeline(
                    &mut run,
                    "CORALOS",
                    format!("sidecar settlement unavailable: {err}"),
                );
            }
        }

        let escrow_account = run
            .settlement
            .as_ref()
            .and_then(|settlement| settlement.escrow_pda.clone());
        let reference = run
            .settlement
            .as_ref()
            .and_then(|settlement| settlement.reference.clone())
            .unwrap_or(reference);
        match triton::rpc::observe_settlement(
            &state.client,
            &state.config,
            reference.clone(),
            escrow_account.clone(),
        )
        .await
        {
            Ok(observation) => {
                if let Some(settlement) = run.settlement.as_mut() {
                    settlement.triton_observed = Some(true);
                    settlement.triton_slot = observation.slot;
                    settlement.explorer_url = observation.slot.map(|slot| {
                        format!("https://explorer.solana.com/block/{slot}?cluster=devnet")
                    });
                }
                coral::market::append_timeline(&mut run, "TRITON", observation.note);
                if let Some(yellowstone) = &state.yellowstone {
                    yellowstone.watch_reference(reference);
                    if let Some(account) = escrow_account {
                        yellowstone.watch_account(account);
                    }
                }
            }
            Err(err) => {
                coral::market::append_timeline(
                    &mut run,
                    "TRITON",
                    format!("chain observer unavailable: {err}"),
                );
            }
        }
    }

    {
        let ledger = state
            .ledger
            .lock()
            .map_err(|_| AppError::Task("ledger lock poisoned".to_string()))?;
        ledger.upsert_run(&run)?;
    }

    for item in &run.timeline {
        let _ = app.emit(
            "market://round",
            MarketRoundEvent {
                run_id: run.run_id.clone(),
                phase: item.label.clone(),
                detail: item.detail.clone(),
                at: item.at.clone(),
            },
        );
    }

    let _ = app.emit(
        "app://notification",
        serde_json::json!({
            "title": "Agent round complete",
            "body": format!("{} produced {}", run.run_id, run.track),
            "ts": now_iso()
        }),
    );

    Ok(run)
}

#[tauri::command]
fn list_runs(state: State<'_, DesktopState>) -> Result<Vec<AgentRun>, AppError> {
    let ledger = state
        .ledger
        .lock()
        .map_err(|_| AppError::Task("ledger lock poisoned".to_string()))?;
    ledger.list_runs()
}

#[tauri::command]
fn get_run(run_id: String, state: State<'_, DesktopState>) -> Result<AgentRun, AppError> {
    let ledger = state
        .ledger
        .lock()
        .map_err(|_| AppError::Task("ledger lock poisoned".to_string()))?;
    ledger.get_run(&run_id)
}

#[tauri::command]
async fn fetch_txline(path: String, state: State<'_, DesktopState>) -> Result<Value, AppError> {
    let jwt = state
        .config
        .txline_guest_jwt
        .as_deref()
        .ok_or_else(|| AppError::Config("TXLINE_GUEST_JWT missing".to_string()))?;
    let token = state
        .config
        .txline_api_token
        .as_deref()
        .ok_or_else(|| AppError::Config("TXLINE_API_TOKEN missing".to_string()))?;
    let url = format!(
        "{}/api/{}",
        state.config.txline_api_origin.trim_end_matches('/'),
        path.trim_start_matches('/')
    );
    let response = state
        .client
        .get(url)
        .bearer_auth(jwt)
        .header("X-Api-Token", token)
        .send()
        .await?
        .error_for_status()?;
    Ok(response.json::<Value>().await?)
}

#[tauri::command]
async fn export_fan_card(
    run_id: String,
    state: State<'_, DesktopState>,
) -> Result<ExportResult, AppError> {
    let run = {
        let ledger = state
            .ledger
            .lock()
            .map_err(|_| AppError::Task("ledger lock poisoned".to_string()))?;
        ledger.get_run(&run_id)?
    };
    tokio::fs::create_dir_all(&state.export_dir).await?;
    let path = state.export_dir.join(format!("{run_id}.txt"));
    let share_text = run
        .delivery
        .as_ref()
        .and_then(|delivery| delivery.fan_copy.clone())
        .unwrap_or_else(|| format!("{} - {}", run.trigger.title, run.trigger.body));
    let contents = format!(
        "World Cup Agent Desk\n\n{}\n\nRun: {}\nTrack: {}\n",
        share_text, run.run_id, run.track
    );
    tokio::fs::write(&path, contents).await?;
    Ok(ExportResult {
        path: path.to_string_lossy().to_string(),
        share_text,
    })
}

#[tauri::command]
async fn yellowstone_status(state: State<'_, DesktopState>) -> Result<String, AppError> {
    if state.yellowstone.is_some() {
        Ok("Yellowstone gRPC observer is running".to_string())
    } else {
        triton::rpc::yellowstone_status(&state.config).await
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let config = AppConfig::load();
            let app_data_dir = app.path().app_data_dir().unwrap_or_else(|_| {
                std::env::current_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .join(".agent-desk")
            });
            std::fs::create_dir_all(&app_data_dir)?;
            let replay_dir = app_data_dir.join("replays");
            let export_dir = app_data_dir.join("exports");
            std::fs::create_dir_all(&replay_dir)?;
            std::fs::create_dir_all(&export_dir)?;

            let ledger = Arc::new(Mutex::new(LedgerStore::open(
                app_data_dir.join("ledger.sqlite3"),
            )?));
            let sidecar_path = resolve_sidecar_path(app, &config);
            let yellowstone_sidecar_path =
                resolve_named_sidecar_path(app, "yellowstone-bridge.mjs");
            let yellowstone =
                if config.triton_grpc_endpoint.is_some() && config.triton_x_token.is_some() {
                    Some(triton::yellowstone::spawn(
                        app.handle().clone(),
                        config.clone(),
                        yellowstone_sidecar_path,
                    ))
                } else {
                    None
                };
            if config.axum_enabled {
                let _ =
                    web::spawn_loopback(config.public(), config.axum_token.clone(), ledger.clone());
            }

            app.manage(DesktopState {
                config,
                client: Client::builder()
                    .timeout(std::time::Duration::from_secs(10))
                    .build()?,
                ledger,
                txline_task: Mutex::new(None),
                yellowstone,
                settlement_bridge: coral::settlement::SettlementBridge::new(sidecar_path),
                replay_dir,
                export_dir,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            list_coral_agents,
            hash_delivery,
            chain_rpc,
            chain_status,
            observe_settlement,
            watch_account,
            watch_program,
            watch_reference,
            start_txline,
            stop_txline,
            run_agent_round,
            list_runs,
            get_run,
            fetch_txline,
            export_fan_card,
            yellowstone_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn sha256_hex(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    hex::encode(hasher.finalize())
}

fn resolve_sidecar_path(app: &tauri::App, config: &AppConfig) -> PathBuf {
    if let Some(path) = config.coralos_sidecar_path.as_deref() {
        return PathBuf::from(path);
    }

    resolve_named_sidecar_path(app, "coralos-bridge.mjs")
}

fn resolve_named_sidecar_path(app: &tauri::App, name: &str) -> PathBuf {
    let dev_path = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("sidecars")
        .join(name);
    if dev_path.exists() {
        return dev_path;
    }

    let resource_dir = app
        .path()
        .resource_dir()
        .unwrap_or_else(|_| PathBuf::from("."));
    let packaged_sidecar = resource_dir.join("sidecars").join(name);
    if packaged_sidecar.exists() {
        return packaged_sidecar;
    }

    resource_dir.join(name)
}
