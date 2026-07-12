//! Optional Axum loopback diagnostics service.
//!
//! Tauri IPC remains the primary webview-to-Rust boundary. This module exposes
//! local-only health/run diagnostics for debugging and must never become a
//! public API surface.

use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, Sse};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;
use tokio_stream::wrappers::IntervalStream;
use tokio_stream::StreamExt;

use crate::config::PublicConfig;
use crate::services::ledger::LedgerStore;

#[derive(Clone)]
struct WebState {
    // Redacted config is safe for health output.
    public_config: PublicConfig,
    // Random per-process bearer token required for every non-health route.
    token: String,
    // Shared ledger handle so diagnostics can inspect persisted runs.
    ledger: Arc<Mutex<LedgerStore>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Health {
    ok: bool,
    product: &'static str,
    version: &'static str,
    config: PublicConfig,
}

pub fn spawn_loopback(
    public_config: PublicConfig,
    token: String,
    ledger: Arc<Mutex<LedgerStore>>,
    bind: String,
    port: u16,
) -> tauri::async_runtime::JoinHandle<()> {
    // Fire-and-forget task; failure to bind diagnostics should not block the
    // desktop app from launching.
    tauri::async_runtime::spawn(async move {
        let state = WebState {
            public_config,
            token,
            ledger,
        };
        let app = Router::new()
            .route("/healthz", get(healthz))
            .route("/api/runs", get(list_runs))
            .route("/api/runs/{id}", get(get_run))
            .route("/api/leaderboard", get(get_leaderboard))
            .route("/api/signals", get(get_signals))
            .route("/api/settlements", get(get_settlements))
            .route("/api/tool-calls", get(get_tool_calls))
            .route("/events", get(events))
            .route("/rpc", post(rpc_placeholder))
            .with_state(state);

        // Binds to `bind:port` from config — defaults to loopback-only
        // (127.0.0.1) so nothing changes unless an operator deliberately sets
        // DESK_AXUM_BIND=0.0.0.0 to let a Docker-spawned CoralOS agent reach
        // this via `host.docker.internal`. Every non-health route still
        // requires the bearer token regardless of bind address.
        let Ok(listener) = tokio::net::TcpListener::bind(format!("{bind}:{port}")).await else {
            return;
        };
        if let Ok(addr) = listener.local_addr() {
            eprintln!("World Cup Agent Desk loopback diagnostics listening on {addr}");
        }
        let _ = axum::serve(listener, app).await;
    })
}

async fn healthz(State(state): State<WebState>) -> impl IntoResponse {
    // Health is intentionally unauthenticated because it contains only redacted
    // public config and product/version data.
    Json(Health {
        ok: true,
        product: "World Cup Agent Desk",
        version: env!("CARGO_PKG_VERSION"),
        config: state.public_config,
    })
}

async fn list_runs(State(state): State<WebState>, headers: HeaderMap) -> impl IntoResponse {
    // Run history is local-user data, so require the per-process bearer token.
    if !authorized(&headers, &state.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let runs = match state.ledger.lock() {
        // The ledger API returns typed AppError; map it into simple HTTP text for
        // diagnostics rather than leaking internals.
        Ok(ledger) => match ledger.list_runs() {
            Ok(runs) => runs,
            Err(err) => {
                return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response()
            }
        },
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "ledger lock poisoned").into_response()
        }
    };
    Json(runs).into_response()
}

async fn get_run(
    State(state): State<WebState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Keep route auth identical to list_runs so tooling can share one client.
    if !authorized(&headers, &state.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let run = match state.ledger.lock() {
        Ok(ledger) => match ledger.get_run(&id) {
            Ok(run) => run,
            Err(err) => return (StatusCode::NOT_FOUND, err.to_string()).into_response(),
        },
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "ledger lock poisoned").into_response()
        }
    };
    Json(run).into_response()
}

// ── Priority-1 agentic tool routes ─────────────────────────────────────────────
//
// Read-only wrappers over LedgerStore, added so a Docker-spawned CoralOS
// agent (currently: fan-pundit-agent) can research past performance before
// narrating, via crates/rig-venice's Get* tools. Same bearer-token gate and
// error-mapping convention as list_runs/get_run above — no new auth pattern,
// no write access, no command execution (see rpc_placeholder).

/// Per-agent leaderboard (win rate, cumulative PnL) — no filters, small and
/// stable enough to return whole.
async fn get_leaderboard(State(state): State<WebState>, headers: HeaderMap) -> impl IntoResponse {
    if !authorized(&headers, &state.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match state.ledger.lock() {
        Ok(ledger) => match ledger.list_agent_leaderboard() {
            Ok(rows) => Json(rows).into_response(),
            Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
        },
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "ledger lock poisoned").into_response(),
    }
}

#[derive(serde::Deserialize)]
struct SignalsQuery {
    fixture_id: Option<i64>,
    limit: Option<i64>,
}

async fn get_signals(
    State(state): State<WebState>,
    Query(q): Query<SignalsQuery>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if !authorized(&headers, &state.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let limit = q.limit.unwrap_or(20).clamp(1, 200);
    match state.ledger.lock() {
        Ok(ledger) => match ledger.list_signal_records(q.fixture_id, limit) {
            Ok(rows) => Json(rows).into_response(),
            Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
        },
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "ledger lock poisoned").into_response(),
    }
}

#[derive(serde::Deserialize)]
struct SettlementsQuery {
    agent_id: Option<String>,
    fixture_id: Option<i64>,
    limit: Option<i64>,
}

async fn get_settlements(
    State(state): State<WebState>,
    Query(q): Query<SettlementsQuery>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if !authorized(&headers, &state.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let limit = q.limit.unwrap_or(20).clamp(1, 200);
    match state.ledger.lock() {
        Ok(ledger) => match ledger.list_settlement_records(q.agent_id.as_deref(), q.fixture_id, limit) {
            Ok(rows) => Json(rows).into_response(),
            Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
        },
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "ledger lock poisoned").into_response(),
    }
}

#[derive(serde::Deserialize)]
struct ToolCallsQuery {
    run_id: Option<String>,
    limit: Option<i64>,
}

async fn get_tool_calls(
    State(state): State<WebState>,
    Query(q): Query<ToolCallsQuery>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if !authorized(&headers, &state.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let limit = q.limit.unwrap_or(20).clamp(1, 200);
    match state.ledger.lock() {
        Ok(ledger) => match ledger.list_tool_call_records(q.run_id.as_deref(), limit) {
            Ok(rows) => Json(rows).into_response(),
            Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
        },
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "ledger lock poisoned").into_response(),
    }
}

async fn events(State(state): State<WebState>, headers: HeaderMap) -> impl IntoResponse {
    if !authorized(&headers, &state.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    // Placeholder SSE proves the diagnostics channel is alive without duplicating
    // the Tauri event bus yet.
    let stream = IntervalStream::new(tokio::time::interval(Duration::from_secs(15))).map(|_| {
        Ok::<Event, Infallible>(
            Event::default()
                .event("status")
                .data("desktop diagnostics event bridge ready"),
        )
    });
    Sse::new(stream).into_response()
}

async fn rpc_placeholder(State(state): State<WebState>, headers: HeaderMap) -> impl IntoResponse {
    if !authorized(&headers, &state.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    // Explicitly disabled: command execution belongs to Tauri IPC unless a
    // future protected loopback API is designed and reviewed.
    (
        StatusCode::NOT_IMPLEMENTED,
        "Use Tauri IPC for primary commands; loopback RPC is intentionally disabled in this build.",
    )
        .into_response()
}

fn authorized(headers: &HeaderMap, token: &str) -> bool {
    // Simple bearer check is enough because the server is loopback-only and the
    // token is random per process.
    headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .map(|value| value == format!("Bearer {token}"))
        .unwrap_or(false)
}
