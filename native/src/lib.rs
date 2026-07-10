//! Tauri desktop backend composition root.
//!
//! `lib.rs` only declares the module tree, builds managed state, and registers
//! commands. Behavior lives in the layers below (see
//! docs/architecture/01-lean-e2e-architecture.md):
//!
//! - `commands/*`: thin IPC adapters exposed to the React webview.
//! - `services/*`: async side-effect units (TxLINE, chain, ledger, proof,
//!   LLM, CoralOS Console, and payments).
//! - `domain/*`: staged deterministic contracts for the room/market/agent
//!   engines.
//! - `event_bus`: the single table of native event topics.
//! - `state`: `DesktopState` plus sidecar path resolution.

mod commands;
mod config;
mod domain;
mod error;
mod event_bus;
mod services;
mod state;
mod types;
mod web;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use reqwest::Client;
use tauri::Manager;

use crate::config::AppConfig;
use crate::services::ledger::LedgerStore;
use crate::services::user_store::UserStore;
use crate::state::{resolve_named_sidecar_path, resolve_resource_dir, DesktopState};

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
            let user_store_path = app_data_dir.join("user_store");
            std::fs::create_dir_all(&user_store_path)?;
            let user_store = Mutex::new(
                UserStore::open(&user_store_path)
                    .map_err(|e| Box::new(std::io::Error::other(e.to_string()))
                        as Box<dyn std::error::Error>)?,
            );
            // Resolve sidecar paths before state registration so configuration
            // errors surface during app setup rather than first proof request.
            let yellowstone_sidecar_path =
                resolve_named_sidecar_path(app, "yellowstone-bridge.mjs");
            let txoracle_validation_sidecar_path =
                resolve_named_sidecar_path(app, "txoracle-validation-bridge.mjs");
            let txoracle_idl_dir = resolve_resource_dir(app, &["vendor", "tx-on-chain", "idl"]);
            let yellowstone =
                if config.grpc_endpoint.is_some() && config.grpc_x_token.is_some() {
                    let handle = services::chain::yellowstone::spawn(
                        app.handle().clone(),
                        config.clone(),
                        yellowstone_sidecar_path,
                    );
                    // Watch the txoracle program from the start so TxLINE proof
                    // roots landing on-chain stream in as chain://tx events.
                    handle.watch_program(config.txline_program_id());
                    Some(handle)
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
                validation_bridge: services::proof::ValidationBridge::new(
                    txoracle_validation_sidecar_path,
                    txoracle_idl_dir,
                ),
                replay_dir,
                export_dir,
                user_store,
                pending_challenges: Mutex::new(std::collections::HashMap::new()),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::config::get_config,
            commands::chain::chain_rpc,
            commands::chain::chain_status,
            commands::chain::observe_settlement,
            commands::chain::watch_account,
            commands::chain::watch_program,
            commands::chain::watch_reference,
            commands::chain::yellowstone_status,
            commands::txline::start_txline,
            commands::txline::stop_txline,
            commands::txline::txline_fixtures_snapshot,
            commands::txline::txline_odds_snapshot,
            commands::txline::txline_odds_updates,
            commands::txline::txline_odds_interval,
            commands::txline::txline_scores_snapshot,
            commands::txline::txline_scores_updates,
            commands::txline::txline_scores_historical,
            commands::txline::txline_scores_interval,
            commands::txline::txline_scores_stat_validation,
            commands::txline::fetch_txline,
            commands::intelligence::run_agent_round,
            commands::intelligence::list_runs,
            commands::intelligence::get_run,
            commands::intelligence::list_coral_agents,
            commands::coral::coral_list_messages,
            commands::coral::agent_list_trace,
            commands::arena::list_arena_positions,
            commands::arena::list_settlement_records,
            commands::arena::list_signal_records,
            commands::arena::get_arena_score,
            commands::arena::list_agent_leaderboard,
            commands::arena::list_tool_call_records,
            commands::arena::get_agent_safety_status,
            commands::arena::list_arena_sessions,
            commands::auth::get_user_profile,
            commands::auth::save_user_profile,
            commands::auth::delete_user_profile,
            commands::auth::issue_auth_challenge,
            commands::auth::request_auth,
            commands::auth::open_phantom_popup,
            commands::payments::create_solana_pay_intent,
            commands::payments::verify_solana_pay_intent,
            commands::payments::list_payment_intents
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
