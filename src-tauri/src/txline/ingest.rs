//! TxLINE live/mock/replay ingestion.
//!
//! Live mode owns TxLINE credentials in Rust, mock mode keeps demos offline, and
//! replay mode re-emits previously recorded JSONL events.

use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::Client;
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;

use crate::config::AppConfig;
use crate::types::{mock_events, now_iso, OddsQuote, Score, TxLineEvent, TxLineEventKind};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct IngestStatus {
    // Source is live/mock/replay so the UI can report the active ingest mode.
    source: String,
    state: String,
    detail: String,
}

pub fn spawn_txline(
    app: AppHandle,
    client: Client,
    config: AppConfig,
    mode: String,
    fixture_id: Option<String>,
    replay_dir: PathBuf,
) -> tauri::async_runtime::JoinHandle<()> {
    // Spawn one independent task per requested mode. lib.rs owns task
    // cancellation when switching modes.
    tauri::async_runtime::spawn(async move {
        match mode.as_str() {
            "live" => live_loop(app, client, config, replay_dir).await,
            "replay" => replay_loop(app, replay_dir, fixture_id).await,
            _ => mock_loop(app).await,
        }
    })
}

async fn mock_loop(app: AppHandle) {
    // Mock mode emits built-in events with a short delay to resemble a live feed.
    emit_status(&app, "mock", "connected", "Rust mock TxLINE stream active");
    for event in mock_events() {
        emit_event(&app, event);
        tokio::time::sleep(Duration::from_millis(800)).await;
    }
    emit_status(&app, "mock", "stopped", "mock stream completed");
}

async fn replay_loop(app: AppHandle, replay_dir: PathBuf, fixture_id: Option<String>) {
    // Replay mode is judging-day insurance: the app can demonstrate real event
    // shapes without a live TxLINE connection.
    emit_status(&app, "replay", "connected", "Rust replay stream active");
    let fixture = fixture_id.unwrap_or_else(|| "default".to_string());
    let path = replay_dir.join(format!("{fixture}.jsonl"));
    let contents = match tokio::fs::read_to_string(&path).await {
        Ok(contents) => contents,
        Err(_) => {
            emit_status(
                &app,
                "replay",
                "reconnecting",
                "no replay found; falling back to built-in mock events",
            );
            mock_loop(app).await;
            return;
        }
    };

    // JSONL keeps replay append simple and lets corrupted lines be reported
    // without dropping the whole replay file.
    for line in contents.lines().filter(|line| !line.trim().is_empty()) {
        match serde_json::from_str::<TxLineEvent>(line) {
            Ok(event) => emit_event(&app, event),
            Err(err) => emit_status(&app, "replay", "reconnecting", &err.to_string()),
        }
        tokio::time::sleep(Duration::from_millis(450)).await;
    }
    emit_status(&app, "replay", "stopped", "replay stream completed");
}

async fn live_loop(app: AppHandle, client: Client, config: AppConfig, replay_dir: PathBuf) {
    let Some(jwt) = config.txline_guest_jwt.clone() else {
        emit_status(
            &app,
            "live",
            "credentials_required",
            "TXLINE_GUEST_JWT missing; live TxLINE cannot start",
        );
        return;
    };
    let Some(token) = config.txline_api_token.clone() else {
        emit_status(
            &app,
            "live",
            "credentials_required",
            "TXLINE_API_TOKEN missing; live TxLINE cannot start",
        );
        return;
    };

    let origin = config.txline_api_origin.trim_end_matches('/').to_string();
    emit_status(
        &app,
        "live",
        "connecting",
        "Rust TxLINE SSE clients starting odds and scores streams",
    );

    let odds = live_stream_loop(
        app.clone(),
        client.clone(),
        replay_dir.clone(),
        origin.clone(),
        jwt.clone(),
        token.clone(),
        "odds",
    );
    let scores = live_stream_loop(app, client, replay_dir, origin, jwt, token, "scores");
    tokio::join!(odds, scores);
}

async fn live_stream_loop(
    app: AppHandle,
    client: Client,
    replay_dir: PathBuf,
    origin: String,
    jwt: String,
    token: String,
    stream: &'static str,
) {
    let mut attempt = 0_u64;
    loop {
        attempt = attempt.saturating_add(1);
        let source = format!("live:{stream}");
        emit_status(
            &app,
            &source,
            "connecting",
            &format!("connecting to TxLINE {stream} SSE attempt {attempt}"),
        );
        match connect_sse_once(&app, &client, &replay_dir, &origin, &jwt, &token, stream).await {
            Ok(()) => emit_status(
                &app,
                &source,
                "reconnecting",
                &format!("TxLINE {stream} stream ended; reconnecting"),
            ),
            Err(err) => emit_status(&app, &source, "reconnecting", &err),
        }
        let backoff_secs = attempt.min(30).max(1);
        tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
    }
}

async fn connect_sse_once(
    app: &AppHandle,
    client: &Client,
    replay_dir: &Path,
    origin: &str,
    jwt: &str,
    token: &str,
    stream: &'static str,
) -> Result<(), String> {
    let stream_url = format!("{}/api/{stream}/stream", origin);
    let response = match client
        .get(stream_url)
        .bearer_auth(jwt)
        .header("X-Api-Token", token)
        .header("Accept", "text/event-stream")
        .header("Cache-Control", "no-cache")
        .send()
        .await
    {
        Ok(response) => response,
        Err(err) => {
            return Err(format!("TxLINE {stream} SSE connection failed: {err}"));
        }
    };

    if !response.status().is_success() {
        return Err(format!("TxLINE {stream} SSE HTTP {}", response.status()));
    }

    emit_status(
        app,
        &format!("live:{stream}"),
        "connected",
        &format!("TxLINE {stream} SSE connected"),
    );
    let mut buffer = String::new();
    let mut byte_stream = response.bytes_stream();
    while let Some(chunk) = byte_stream.next().await {
        let Ok(chunk) = chunk else {
            return Err("TxLINE stream chunk failed".to_string());
        };
        buffer.push_str(&String::from_utf8_lossy(&chunk));
        while let Some((index, delimiter_len)) = find_sse_separator(&buffer) {
            let block = buffer[..index].to_string();
            buffer = buffer[index + delimiter_len..].to_string();
            if let Some(event) = parse_sse_block(stream, &block) {
                append_replay(replay_dir, &event).await;
                emit_event(app, event);
            }
        }
    }
    Ok(())
}

fn parse_sse_block(stream: &str, block: &str) -> Option<TxLineEvent> {
    let mut data_lines = Vec::new();
    let mut sse_event: Option<String> = None;
    let mut sse_id: Option<String> = None;

    for line in block.lines() {
        let line = line.trim_start();
        if let Some(data) = line.strip_prefix("data:") {
            data_lines.push(data.trim_start().to_string());
        } else if let Some(event) = line.strip_prefix("event:") {
            sse_event = Some(event.trim().to_string());
        } else if let Some(id) = line.strip_prefix("id:") {
            sse_id = Some(id.trim().to_string());
        }
    }

    let payload = data_lines.join("\n");
    if payload.trim().is_empty() || payload.trim() == "[DONE]" {
        return None;
    }
    let raw = serde_json::from_str::<Value>(&payload).ok()?;
    let fixture_id = extract_u64(
        &raw,
        &[
            "fixtureId",
            "fixture_id",
            "fixture",
            "matchId",
            "gameId",
            "id",
        ],
    )
    .unwrap_or(0);
    let title = extract_string(&raw, &["title", "headline", "event", "type"])
        .or(sse_event)
        .unwrap_or_else(|| format!("{stream} update"));
    let body = extract_string(&raw, &["body", "message", "description", "summary"])
        .unwrap_or_else(|| "Live TxLINE SSE event received by Rust".to_string());
    let odds = (stream == "odds")
        .then(|| parse_odds(&raw, fixture_id))
        .flatten();
    let score = (stream == "scores").then(|| parse_score(&raw)).flatten();
    let kind = event_kind(stream, &title, &raw);

    Some(TxLineEvent {
        id: sse_id.unwrap_or_else(|| {
            extract_string(&raw, &["eventId", "event_id", "uuid"])
                .unwrap_or_else(|| format!("{stream}-{}", uuid::Uuid::new_v4()))
        }),
        kind,
        fixture_id,
        title,
        body,
        ts: now_iso(),
        raw: Some(raw),
        odds,
        score,
        proof: None,
    })
}

fn find_sse_separator(buffer: &str) -> Option<(usize, usize)> {
    let lf = buffer.find("\n\n").map(|index| (index, 2));
    let crlf = buffer.find("\r\n\r\n").map(|index| (index, 4));
    match (lf, crlf) {
        (Some(a), Some(b)) => Some(if a.0 < b.0 { a } else { b }),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn event_kind(stream: &str, title: &str, raw: &Value) -> TxLineEventKind {
    let needle = format!(
        "{} {}",
        title.to_ascii_lowercase(),
        extract_string(raw, &["kind", "eventType", "event_type", "status"])
            .unwrap_or_default()
            .to_ascii_lowercase()
    );
    if needle.contains("goal") {
        TxLineEventKind::Goal
    } else if needle.contains("red_card") || needle.contains("red card") {
        TxLineEventKind::RedCard
    } else if needle.contains("final") {
        TxLineEventKind::FinalWhistle
    } else if stream == "odds" {
        TxLineEventKind::OddsUpdate
    } else {
        TxLineEventKind::ScoreUpdate
    }
}

fn parse_odds(raw: &Value, fixture_id: u64) -> Option<Vec<OddsQuote>> {
    let values = raw
        .as_array()
        .cloned()
        .or_else(|| raw.get("odds").and_then(Value::as_array).cloned())
        .or_else(|| raw.get("quotes").and_then(Value::as_array).cloned())
        .or_else(|| raw.get("markets").and_then(Value::as_array).cloned())?;

    let quotes = values
        .iter()
        .filter_map(|item| {
            let decimal = extract_f64(item, &["decimal", "price", "odds"])?;
            if decimal <= 1.0 {
                return None;
            }
            Some(OddsQuote {
                fixture_id: extract_u64(item, &["fixtureId", "fixture_id"]).unwrap_or(fixture_id),
                outcome: extract_string(item, &["outcome", "selection", "name", "side"])
                    .unwrap_or_else(|| "unknown".to_string()),
                decimal,
                implied_probability: 1.0 / decimal,
                source: extract_string(item, &["source", "book", "bookmaker"]),
                ts: extract_string(item, &["ts", "timestamp"]).unwrap_or_else(now_iso),
            })
        })
        .collect::<Vec<_>>();

    (!quotes.is_empty()).then_some(quotes)
}

fn parse_score(raw: &Value) -> Option<Score> {
    if let Some(score) = raw.get("score") {
        if let Some(parsed) = parse_score_object(score) {
            return Some(parsed);
        }
    }
    parse_score_object(raw)
}

fn parse_score_object(value: &Value) -> Option<Score> {
    let home = extract_i64(value, &["home", "homeScore", "home_score", "homeGoals"])?;
    let away = extract_i64(value, &["away", "awayScore", "away_score", "awayGoals"])?;
    Some(Score { home, away })
}

fn extract_string(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(ToString::to_string)
    })
}

fn extract_u64(value: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter().find_map(|key| {
        value.get(*key).and_then(|item| {
            item.as_u64()
                .or_else(|| item.as_str().and_then(|text| text.parse::<u64>().ok()))
        })
    })
}

fn extract_i64(value: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter().find_map(|key| {
        value.get(*key).and_then(|item| {
            item.as_i64()
                .or_else(|| item.as_str().and_then(|text| text.parse::<i64>().ok()))
        })
    })
}

fn extract_f64(value: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|key| {
        value.get(*key).and_then(|item| {
            item.as_f64()
                .or_else(|| item.as_str().and_then(|text| text.parse::<f64>().ok()))
        })
    })
}

async fn append_replay(replay_dir: &Path, event: &TxLineEvent) {
    // Replay append is best-effort. Failure should not prevent live events from
    // reaching the UI.
    if tokio::fs::create_dir_all(replay_dir).await.is_err() {
        return;
    }
    append_replay_file(replay_dir.join("default.jsonl"), event).await;
    if event.fixture_id > 0 {
        append_replay_file(
            replay_dir.join(format!("{}.jsonl", event.fixture_id)),
            event,
        )
        .await;
    }
}

async fn append_replay_file(path: PathBuf, event: &TxLineEvent) {
    let Ok(mut file) = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await
    else {
        return;
    };
    if let Ok(line) = serde_json::to_string(event) {
        let _ = file.write_all(line.as_bytes()).await;
        let _ = file.write_all(b"\n").await;
    }
}

fn emit_event(app: &AppHandle, event: TxLineEvent) {
    // txline://event is the canonical webview feed event.
    let _ = app.emit("txline://event", event);
}

fn emit_status(app: &AppHandle, source: &str, state: &str, detail: &str) {
    // Status events share a shape with Yellowstone status updates.
    let _ = app.emit(
        "ingest://status",
        IngestStatus {
            source: source.to_string(),
            state: state.to_string(),
            detail: detail.to_string(),
        },
    );
}
