use std::time::Instant;

use reqwest::Client;
use serde_json::{json, Value};

use crate::config::AppConfig;
use crate::error::AppError;
use crate::types::{now_iso, ChainStatus, Cluster, TritonObservation};

const ALLOWED_RPC_METHODS: &[&str] = &[
    "getSlot",
    "getVersion",
    "getBalance",
    "getLatestBlockhash",
    "getSignaturesForAddress",
    "getAccountInfo",
    "getTransaction",
];

pub fn validate_rpc_method(method: &str) -> Result<(), AppError> {
    if ALLOWED_RPC_METHODS.contains(&method) {
        Ok(())
    } else {
        Err(AppError::InvalidInput(format!(
            "RPC method {method} is not allowed"
        )))
    }
}

pub async fn triton_rpc(
    client: &Client,
    config: &AppConfig,
    cluster: Cluster,
    method: &str,
    params: Value,
) -> Result<Value, AppError> {
    validate_rpc_method(method)?;
    let endpoint = config
        .triton_endpoint(cluster)
        .ok_or_else(|| AppError::Config(format!("{cluster:?} Triton RPC endpoint missing")))?;
    let token = config
        .triton_token(cluster)
        .ok_or_else(|| AppError::Config(format!("{cluster:?} Triton x-token missing")))?;

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
    let started = Instant::now();
    let slot = triton_rpc(client, config, cluster, "getSlot", Value::Array(vec![]))
        .await?
        .as_u64()
        .unwrap_or(0);
    let latency_ms = started.elapsed().as_millis();

    let version = triton_rpc(client, config, cluster, "getVersion", Value::Array(vec![])).await?;
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
) -> Result<TritonObservation, AppError> {
    let slot = triton_rpc(
        client,
        config,
        Cluster::Devnet,
        "getSlot",
        Value::Array(vec![]),
    )
    .await?
    .as_u64();
    let blockhash_info = triton_rpc(
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

    let signature = if let Some(account) = escrow_account.as_deref() {
        let sigs = triton_rpc(
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

    Ok(TritonObservation {
        kind: "account_update".to_string(),
        signature,
        slot,
        blockhash,
        account: escrow_account,
        program_id: None,
        note: format!(
            "Triton devnet observed {reference} at slot {}",
            slot.map_or_else(|| "unknown".to_string(), |value| value.to_string())
        ),
    })
}

pub async fn yellowstone_status(config: &AppConfig) -> Result<String, AppError> {
    if config.triton_grpc_endpoint.is_some() && config.triton_x_token.is_some() {
        Ok("configured; Rust-managed Yellowstone gRPC sidecar streams slots, accounts, and transactions".to_string())
    } else {
        Err(AppError::Config(
            "TRITON_GRPC_ENDPOINT and TRITON_X_TOKEN are required for Yellowstone streams"
                .to_string(),
        ))
    }
}
