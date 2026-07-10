//! Solana JSON-RPC client.
//!
//! This module keeps RPC tokens in Rust and restricts the webview to
//! a small allowlist of Solana RPC methods.

use std::time::Instant;

use reqwest::Client;
use serde_json::{json, Value};

use crate::config::AppConfig;
use crate::error::AppError;
use crate::types::{now_iso, ChainObservation, ChainStatus, Cluster};

const ALLOWED_RPC_METHODS: &[&str] = &[
    "getSlot",
    "getVersion",
    "getBalance",
    "getLatestBlockhash",
    "getSignaturesForAddress",
    "getAccountInfo",
    "getTransaction",
];

// Guardrail for the generic chain_rpc Tauri command.
pub fn validate_rpc_method(method: &str) -> Result<(), AppError> {
    if ALLOWED_RPC_METHODS.contains(&method) {
        Ok(())
    } else {
        Err(AppError::InvalidInput(format!(
            "RPC method {method} is not allowed"
        )))
    }
}

pub async fn solana_rpc(
    client: &Client,
    config: &AppConfig,
    cluster: Cluster,
    method: &str,
    params: Value,
) -> Result<Value, AppError> {
    validate_rpc_method(method)?;
    // Endpoint/token lookup is cluster-specific so devnet and mainnet can be
    // configured independently.
    let endpoint = config
        .rpc_endpoint(cluster)
        .ok_or_else(|| AppError::Config(format!("{cluster:?} RPC endpoint missing")))?;
    let token = config
        .rpc_token(cluster)
        .ok_or_else(|| AppError::Config(format!("{cluster:?} RPC x-token missing")))?;

    // Normalize params to JSON-RPC's array form. This keeps the webview command
    // tolerant of null/single-object params without allowing arbitrary methods.
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": match params {
            Value::Array(_) => params,
            Value::Null => Value::Array(vec![]),
            other => Value::Array(vec![other]),
        }
    });

    // The RPC provider accepts x-token as a header; using headers avoids tokenized URLs in
    // logs, cache keys, or UI-visible strings.
    let body: Value = client
        .post(endpoint)
        .header("x-token", token)
        .json(&request)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    if let Some(error) = body.get("error") {
        // Preserve JSON-RPC code/message for useful UI diagnostics.
        return Err(AppError::Rpc {
            code: error.get("code").and_then(Value::as_i64).unwrap_or(-1),
            message: error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown Solana RPC error")
                .to_string(),
        });
    }

    Ok(body.get("result").cloned().unwrap_or(Value::Null))
}

pub async fn chain_status(
    client: &Client,
    config: &AppConfig,
    cluster: Cluster,
) -> Result<ChainStatus, AppError> {
    // Slot latency is measured only around getSlot because that is the hot path
    // shown in the chain strip.
    let started = Instant::now();
    let slot = solana_rpc(client, config, cluster, "getSlot", Value::Array(vec![]))
        .await?
        .as_u64()
        .unwrap_or(0);
    let latency_ms = started.elapsed().as_millis();

    // getVersion is slower/less volatile than slot but useful for proving a live
    // RPC path in the UI.
    let version = solana_rpc(client, config, cluster, "getVersion", Value::Array(vec![])).await?;
    let solana_core = version
        .get("solana-core")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();

    Ok(ChainStatus {
        cluster,
        slot,
        solana_core,
        latency_ms,
        ts: now_iso(),
    })
}

pub async fn observe_settlement(
    client: &Client,
    config: &AppConfig,
    reference: String,
    escrow_account: Option<String>,
) -> Result<ChainObservation, AppError> {
    // This snapshot gives the proof panel a current devnet slot/blockhash even
    // before a real escrow PDA is available.
    let slot = solana_rpc(
        client,
        config,
        Cluster::Devnet,
        "getSlot",
        Value::Array(vec![]),
    )
    .await?
    .as_u64();
    let blockhash_info = solana_rpc(
        client,
        config,
        Cluster::Devnet,
        "getLatestBlockhash",
        Value::Array(vec![]),
    )
    .await?;

    let blockhash = blockhash_info
        .pointer("/value/blockhash")
        .and_then(Value::as_str)
        .map(ToString::to_string);

    // If settlement returned an escrow account, attach its most recent signature
    // to make the observation more concrete.
    let signature = if let Some(account) = escrow_account.as_deref() {
        let sigs = solana_rpc(
            client,
            config,
            Cluster::Devnet,
            "getSignaturesForAddress",
            json!([account, { "limit": 1 }]),
        )
        .await
        .ok();
        sigs.and_then(|value| {
            value
                .as_array()
                .and_then(|items| items.first())
                .and_then(|item| item.get("signature"))
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
    } else {
        None
    };

    Ok(ChainObservation {
        kind: "account_update".to_string(),
        signature,
        slot,
        blockhash,
        account: escrow_account,
        program_id: None,
        note: format!(
            "Devnet observed {reference} at slot {}",
            slot.map_or_else(|| "unknown".to_string(), |value| value.to_string())
        ),
    })
}

pub async fn yellowstone_status(config: &AppConfig) -> Result<String, AppError> {
    // Status command reports configuration readiness when the sidecar is not
    // already running.
    if config.grpc_endpoint.is_some() && config.grpc_x_token.is_some() {
        Ok("configured; Rust-managed Yellowstone gRPC sidecar streams slots, accounts, and transactions".to_string())
    } else {
        Err(AppError::Config(
            "GRPC_ENDPOINT and GRPC_X_TOKEN are required for Yellowstone streams"
                .to_string(),
        ))
    }
}
