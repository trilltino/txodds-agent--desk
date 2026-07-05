//! Tauri desktop backend.
//!
//! This module wires the privileged Rust side of the app: config, HTTP client,
//! SQLite ledger, TxLINE ingestion, Triton RPC/Yellowstone observation, CoralOS
//! settlement sidecars, and the IPC commands exposed to the React webview.

mod config;
mod coral;
mod error;
mod ledger;
mod solana_pay;
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
use crate::solana_pay::SolanaPayIntent;
use crate::types::{
    now_iso, AgentRun, ChainStatus, Cluster, MarketRoundEvent, SettlementReceipt, TrackMode,
    TritonObservation, TxLineEvent, VerdictStatus,
};

struct DesktopState {
    // Full config may contain secrets; only PublicConfig is returned to JS.
    config: AppConfig,
    // Shared HTTP client for Triton, TxLINE, and sidecar-adjacent calls.
    client: Client,
    // SQLite is protected by a Mutex because Tauri commands/background tasks can
    // access it concurrently, while rusqlite::Connection itself is synchronous.
    ledger: Arc<Mutex<LedgerStore>>,
    // Current TxLINE ingest task. Starting a new mode aborts the previous one.
    txline_task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    // Optional Yellowstone supervisor; absent when gRPC config is missing.
    yellowstone: Option<triton::yellowstone::YellowstoneHandle>,
    // CoralOS sidecar bridge used after a run is verified.
    settlement_bridge: coral::settlement::SettlementBridge,
    // App-data directories, not repo paths, for durable user/runtime output.
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
    // Never serialize AppConfig directly; it may contain tokens/keypaths.
    state.config.public()
}

#[tauri::command]
fn list_coral_agents() -> Vec<coral::agents::CoralAgentManifest> {
    // Current registry is built-in Rust metadata mirrored by coral-agents TOML.
    coral::agents::built_in_agents()
}

#[tauri::command]
fn hash_delivery(payload: String) -> HashReceipt {
    // Stable hash/reference helper used by the webview for local artifacts and
    // by future settlement/proof flows.
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
    // triton::rpc validates the method allowlist before any network call.
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
    // Echo status as an event so UI subscribers and one-shot command callers see
    // consistent chain health.
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
    // Register continuous watches first when Yellowstone is available, then take
    // an immediate RPC snapshot for command response/persistence.
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
    // Watch commands are thin IPC adapters around the Yellowstone supervisor.
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
    // Only one TxLINE task should own event emission at a time.
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
    // Abort is acceptable for ingest streams because events are append-only and
    // replay writes happen before emission.
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
    // The market engine produces a complete deterministic run first. Settlement
    // and chain observation enrich it afterwards.
    let mut run = coral::market::run_round(trigger, track);

    if verifier_passed(&run) {
        match solana_pay::create_intent(&state.config, &run) {
            Ok(intent) => {
                if let Some(yellowstone) = &state.yellowstone {
                    yellowstone.watch_reference(intent.reference.clone());
                }
                merge_receipt(&mut run, solana_pay::receipt_from_intent(&intent));
                coral::market::append_timeline(
                    &mut run,
                    "SOLANA_PAY",
                    format!(
                        "created devnet transfer request for {} SOL with reference {}",
                        intent.amount_sol, intent.reference
                    ),
                );
                {
                    let ledger = state
                        .ledger
                        .lock()
                        .map_err(|_| AppError::Task("ledger lock poisoned".to_string()))?;
                    ledger.upsert_payment_intent(&intent)?;
                }
                let _ = app.emit("pay://intent", &intent);
            }
            Err(err) => {
                coral::market::append_timeline(
                    &mut run,
                    "SOLANA_PAY",
                    format!("payment intent unavailable: {err}"),
                );
            }
        }
    }

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
                // Settlement success updates both the run timeline and live UI
                // event stream. The webview sees receipts, not secrets.
                let settled_reference = receipt
                    .reference
                    .clone()
                    .or_else(|| {
                        run.settlement
                            .as_ref()
                            .and_then(|settlement| settlement.reference.clone())
                    })
                    .unwrap_or_else(|| reference.clone());
                let detail = format!("CoralOS sidecar settled reference {}", settled_reference);
                merge_receipt(&mut run, receipt);
                coral::market::append_timeline(&mut run, "CORALOS", detail);
                if let Some(receipt) = run.settlement.as_ref() {
                    let _ = app.emit("settle://receipt", receipt);
                    if let Some(yellowstone) = &state.yellowstone {
                        if let Some(account) = receipt.escrow_pda.clone() {
                            yellowstone.watch_account(account);
                        }
                        if let Some(reference) = receipt.reference.clone() {
                            yellowstone.watch_reference(reference);
                        }
                    }
                }
            }
            Err(err) => {
                // Settlement is non-fatal for demoability: the run remains
                // inspectable and records why CoralOS was unavailable.
                coral::market::append_timeline(
                    &mut run,
                    "CORALOS",
                    format!("sidecar settlement unavailable: {err}"),
                );
            }
        }

        if let Some(payment_reference) = run
            .settlement
            .as_ref()
            .and_then(|settlement| settlement.payment_reference.clone())
        {
            let intent = {
                let ledger = state
                    .ledger
                    .lock()
                    .map_err(|_| AppError::Task("ledger lock poisoned".to_string()))?;
                ledger
                    .get_payment_intent_by_reference(&payment_reference)
                    .ok()
            };
            if let Some(intent) = intent {
                match solana_pay::verify_intent(&state.client, &state.config, intent).await {
                    Ok(updated) => {
                        merge_receipt(&mut run, solana_pay::receipt_from_intent(&updated));
                        {
                            let ledger = state
                                .ledger
                                .lock()
                                .map_err(|_| AppError::Task("ledger lock poisoned".to_string()))?;
                            ledger.upsert_payment_intent(&updated)?;
                        }
                        coral::market::append_timeline(
                            &mut run,
                            "SOLANA_PAY",
                            format!(
                                "{} payment reference {}",
                                updated.status_text(),
                                updated.reference
                            ),
                        );
                        let _ = app.emit("pay://status", &updated);
                    }
                    Err(err) => {
                        coral::market::append_timeline(
                            &mut run,
                            "SOLANA_PAY",
                            format!("payment reference verification unavailable: {err}"),
                        );
                    }
                }
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
                // Triton observation turns a local reference into a chain-stamped
                // proof panel entry.
                if let Some(settlement) = run.settlement.as_mut() {
                    settlement.triton_observed = Some(
                        observation.signature.is_some() || settlement.payment_signature.is_some(),
                    );
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
                // Keep the run even when RPC is unreachable; the ledger should
                // reflect the attempted observation.
                coral::market::append_timeline(
                    &mut run,
                    "TRITON",
                    format!("chain observer unavailable: {err}"),
                );
            }
        }
    }

    {
        // Persist after settlement/observation enrichment so restart history
        // contains the best available audit trail.
        let ledger = state
            .ledger
            .lock()
            .map_err(|_| AppError::Task("ledger lock poisoned".to_string()))?;
        ledger.upsert_run(&run)?;
    }

    for item in &run.timeline {
        // Replay the complete timeline as live events. Newer UI can animate
        // phases, while current UI receives the final run response.
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
        // Notification event is app-internal for now; native notification UI can
        // subscribe to the same semantic payload later.
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
    // History is loaded from SQLite rather than webview memory.
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
fn create_solana_pay_intent(
    run_id: String,
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<SolanaPayIntent, AppError> {
    let mut run = {
        let ledger = state
            .ledger
            .lock()
            .map_err(|_| AppError::Task("ledger lock poisoned".to_string()))?;
        ledger.get_run(&run_id)?
    };

    if !verifier_passed(&run) {
        return Err(AppError::InvalidInput(
            "verifier must pass before Solana Pay intent creation".to_string(),
        ));
    }

    let intent = solana_pay::create_intent(&state.config, &run)?;
    let receipt = solana_pay::receipt_from_intent(&intent);
    merge_receipt(&mut run, receipt);
    coral::market::append_timeline(
        &mut run,
        "SOLANA_PAY",
        format!("created devnet transfer request {}", intent.reference),
    );

    {
        let ledger = state
            .ledger
            .lock()
            .map_err(|_| AppError::Task("ledger lock poisoned".to_string()))?;
        ledger.upsert_payment_intent(&intent)?;
        ledger.upsert_run(&run)?;
    }

    let _ = app.emit("pay://intent", &intent);
    if let Some(settlement) = run.settlement.as_ref() {
        let _ = app.emit("settle://receipt", settlement);
    }
    Ok(intent)
}

#[tauri::command]
async fn verify_solana_pay_intent(
    reference: String,
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<SolanaPayIntent, AppError> {
    let intent = {
        let ledger = state
            .ledger
            .lock()
            .map_err(|_| AppError::Task("ledger lock poisoned".to_string()))?;
        ledger.get_payment_intent_by_reference(&reference)?
    };
    let updated = solana_pay::verify_intent(&state.client, &state.config, intent).await?;

    {
        let ledger = state
            .ledger
            .lock()
            .map_err(|_| AppError::Task("ledger lock poisoned".to_string()))?;
        ledger.upsert_payment_intent(&updated)?;
        if let Ok(mut run) = ledger.get_run(&updated.run_id) {
            merge_receipt(&mut run, solana_pay::receipt_from_intent(&updated));
            coral::market::append_timeline(
                &mut run,
                "SOLANA_PAY",
                format!("{} reference {}", updated.status_text(), updated.reference),
            );
            ledger.upsert_run(&run)?;
            if let Some(settlement) = run.settlement.as_ref() {
                let _ = app.emit("settle://receipt", settlement);
            }
        }
    }

    let _ = app.emit("pay://status", &updated);
    Ok(updated)
}

#[tauri::command]
fn list_payment_intents(
    run_id: Option<String>,
    state: State<'_, DesktopState>,
) -> Result<Vec<SolanaPayIntent>, AppError> {
    let ledger = state
        .ledger
        .lock()
        .map_err(|_| AppError::Task("ledger lock poisoned".to_string()))?;
    ledger.list_payment_intents(run_id.as_deref())
}

#[tauri::command]
async fn txline_fixtures_snapshot(
    start_epoch_day: Option<u64>,
    competition_id: Option<u64>,
    state: State<'_, DesktopState>,
) -> Result<Value, AppError> {
    let mut query = Vec::new();
    push_query(&mut query, "startEpochDay", start_epoch_day);
    push_query(&mut query, "competitionId", competition_id);
    txline::api::authenticated_get(&state.client, &state.config, "api/fixtures/snapshot", query)
        .await
}

#[tauri::command]
async fn txline_odds_snapshot(
    fixture_id: u64,
    as_of: Option<u64>,
    state: State<'_, DesktopState>,
) -> Result<Value, AppError> {
    let mut query = Vec::new();
    push_query(&mut query, "asOf", as_of);
    txline::api::authenticated_get(
        &state.client,
        &state.config,
        &format!("api/odds/snapshot/{fixture_id}"),
        query,
    )
    .await
}

#[tauri::command]
async fn txline_odds_updates(
    fixture_id: u64,
    state: State<'_, DesktopState>,
) -> Result<Value, AppError> {
    txline::api::authenticated_get(
        &state.client,
        &state.config,
        &format!("api/odds/updates/{fixture_id}"),
        vec![],
    )
    .await
}

#[tauri::command]
async fn txline_odds_interval(
    epoch_day: u64,
    hour_of_day: u64,
    interval: u64,
    state: State<'_, DesktopState>,
) -> Result<Value, AppError> {
    txline::api::authenticated_get(
        &state.client,
        &state.config,
        &format!("api/odds/updates/{epoch_day}/{hour_of_day}/{interval}"),
        vec![],
    )
    .await
}

#[tauri::command]
async fn txline_scores_snapshot(
    fixture_id: u64,
    as_of: Option<u64>,
    state: State<'_, DesktopState>,
) -> Result<Value, AppError> {
    let mut query = Vec::new();
    push_query(&mut query, "asOf", as_of);
    txline::api::authenticated_get(
        &state.client,
        &state.config,
        &format!("api/scores/snapshot/{fixture_id}"),
        query,
    )
    .await
}

#[tauri::command]
async fn txline_scores_updates(
    fixture_id: u64,
    state: State<'_, DesktopState>,
) -> Result<Value, AppError> {
    txline::api::authenticated_get(
        &state.client,
        &state.config,
        &format!("api/scores/updates/{fixture_id}"),
        vec![],
    )
    .await
}

#[tauri::command]
async fn txline_scores_historical(
    fixture_id: u64,
    state: State<'_, DesktopState>,
) -> Result<Value, AppError> {
    txline::api::authenticated_get(
        &state.client,
        &state.config,
        &format!("api/scores/historical/{fixture_id}"),
        vec![],
    )
    .await
}

#[tauri::command]
async fn txline_scores_interval(
    epoch_day: u64,
    hour_of_day: u64,
    interval: u64,
    state: State<'_, DesktopState>,
) -> Result<Value, AppError> {
    txline::api::authenticated_get(
        &state.client,
        &state.config,
        &format!("api/scores/updates/{epoch_day}/{hour_of_day}/{interval}"),
        vec![],
    )
    .await
}

#[tauri::command]
async fn txline_scores_stat_validation(
    fixture_id: u64,
    seq: u64,
    stat_key: Option<u64>,
    stat_key2: Option<u64>,
    stat_keys: Option<String>,
    state: State<'_, DesktopState>,
) -> Result<Value, AppError> {
    let mut query = vec![
        ("fixtureId", fixture_id.to_string()),
        ("seq", seq.to_string()),
    ];
    push_query(&mut query, "statKey", stat_key);
    push_query(&mut query, "statKey2", stat_key2);
    if let Some(stat_keys) = stat_keys.filter(|value| !value.trim().is_empty()) {
        query.push(("statKeys", stat_keys));
    }
    txline::api::authenticated_get(
        &state.client,
        &state.config,
        "api/scores/stat-validation",
        query,
    )
    .await
}

#[tauri::command]
async fn fetch_txline(path: String, state: State<'_, DesktopState>) -> Result<Value, AppError> {
    // Escape hatch for backend-owned TxLINE reads. It is intentionally
    // allowlisted to documented GET data/proof endpoints; streams use
    // start_txline, and auth/token activation cannot be reached from here.
    txline::api::authenticated_get(&state.client, &state.config, &path, vec![]).await
}

#[tauri::command]
async fn export_fan_card(
    run_id: String,
    state: State<'_, DesktopState>,
) -> Result<ExportResult, AppError> {
    // Export uses the ledger as source of truth so the webview cannot write
    // arbitrary filesystem data.
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
    // Explicit status command helps distinguish "not configured" from "running".
    if state.yellowstone.is_some() {
        Ok("Yellowstone gRPC observer is running".to_string())
    } else {
        triton::rpc::yellowstone_status(&state.config).await
    }
}

pub fn run() {
    // Builder setup is the root composition point for plugins, managed state,
    // commands, and background services.
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let config = AppConfig::load();
            // App-data is the durable runtime home for ledger, replays, exports,
            // and any future per-user state. Repo paths are for source only.
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
            // Resolve sidecar paths before state registration so configuration
            // errors surface during app setup rather than first settlement.
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
            // Optional Axum diagnostics bind to loopback only and remain
            // secondary to Tauri IPC.
            if config.axum_enabled {
                std::mem::drop(web::spawn_loopback(
                    config.public(),
                    config.axum_token.clone(),
                    ledger.clone(),
                ));
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
            create_solana_pay_intent,
            verify_solana_pay_intent,
            list_payment_intents,
            txline_fixtures_snapshot,
            txline_odds_snapshot,
            txline_odds_updates,
            txline_odds_interval,
            txline_scores_snapshot,
            txline_scores_updates,
            txline_scores_historical,
            txline_scores_interval,
            txline_scores_stat_validation,
            fetch_txline,
            export_fan_card,
            yellowstone_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn sha256_hex(text: &str) -> String {
    // Hex SHA-256 is used for stable delivery references across Rust and JS.
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    hex::encode(hasher.finalize())
}

fn push_query(query: &mut Vec<(&'static str, String)>, name: &'static str, value: Option<u64>) {
    if let Some(value) = value {
        query.push((name, value.to_string()));
    }
}

fn verifier_passed(run: &AgentRun) -> bool {
    run.verdict
        .as_ref()
        .map(|verdict| matches!(verdict.status, VerdictStatus::Pass))
        .unwrap_or(false)
}

fn merge_receipt(run: &mut AgentRun, mut incoming: SettlementReceipt) {
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
        if incoming.triton_observed.is_none() {
            incoming.triton_observed = existing.triton_observed;
        }
        if incoming.triton_slot.is_none() {
            incoming.triton_slot = existing.triton_slot;
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

fn resolve_sidecar_path(app: &tauri::App, config: &AppConfig) -> PathBuf {
    // Explicit config override wins for local CoralOS experimentation.
    if let Some(path) = config.coralos_sidecar_path.as_deref() {
        return PathBuf::from(path);
    }

    resolve_named_sidecar_path(app, "coralos-bridge.mjs")
}

fn resolve_named_sidecar_path(app: &tauri::App, name: &str) -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    // Development layout after workspace compartmentalization.
    let dev_path = cwd.join("runtime").join("sidecars").join(name);
    if dev_path.exists() {
        return dev_path;
    }

    // Legacy fallback keeps older local builds/packages from breaking while the
    // repo finishes migrating away from root sidecars/.
    let legacy_dev_path = cwd.join("sidecars").join(name);
    if legacy_dev_path.exists() {
        return legacy_dev_path;
    }

    let resource_dir = app
        .path()
        .resource_dir()
        .unwrap_or_else(|_| PathBuf::from("."));
    // Packaged resource layout mirrors the source runtime/sidecars directory.
    let packaged_sidecar = resource_dir.join("runtime").join("sidecars").join(name);
    if packaged_sidecar.exists() {
        return packaged_sidecar;
    }

    // Legacy packaged resource fallback.
    let legacy_packaged_sidecar = resource_dir.join("sidecars").join(name);
    if legacy_packaged_sidecar.exists() {
        return legacy_packaged_sidecar;
    }

    resource_dir.join(name)
}
