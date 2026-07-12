//! Autonomous live-trigger loop.
//!
//! Polls TxLINE's fixtures snapshot every `config.autonomous_poll_secs`,
//! diffs each in-kickoff-window fixture's 1X2 odds against the previous
//! poll, and — when a move crosses `config.odds_move_trigger_pct` — calls
//! the *same* `run_match_intelligence_round` the chat's Analyze button
//! calls. This module only decides *when* a round happens; the
//! deterministic decision logic inside that function is completely
//! unchanged and has no idea whether a human or this loop triggered it.
//!
//! Rate-limited per fixture (`MIN_RETRIGGER_INTERVAL`) so a choppy market
//! can't spawn a round every poll cycle. Toggleable at runtime via
//! `commands::autonomous::set_autonomous_loop_enabled`; starts enabled per
//! `config.autonomous_enabled` (default true) so the app acts without
//! anyone touching it — that observable (a round appearing in chat with no
//! click) is the actual bar for "autonomous," not the presence of this code.
//!
//! ARENA-AUTONOMY-PLAN.md Priority A / E2E-AGENTIC-GAPS-PLAN.md #2.

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use serde_json::Value;
use tauri::{AppHandle, Manager};

use crate::error::AppError;
use crate::services::agent::runtime::run_match_intelligence_round;
use crate::services::backtest::{parse_market_rows, ts_to_iso, OddsTick};
use crate::services::txline::api::authenticated_get;
use crate::state::DesktopState;
use crate::types::{now_iso, OddsQuote, TrackMode, TxLineEvent, TxLineEventKind};

/// Minimum time between two autonomously-triggered rounds on the same
/// fixture. An odds market can tick many times an hour; this is the
/// difference between "reacts to real movement" and "spams a round every
/// poll cycle whenever the line is choppy."
const MIN_RETRIGGER_INTERVAL: Duration = Duration::from_secs(3600);

/// A fixture is eligible for autonomous polling if it's within this window
/// of its own kickoff — matches `services::backtest`'s post-kickoff window,
/// which was verified live to comfortably cover full time + extra time.
/// Deliberately NOT based on TxLINE's numeric `GameState` fixture-snapshot
/// field, whose value mapping was never confirmed against live data in this
/// codebase (unlike the score-timeline `GameState` string field, which is).
const KICKOFF_WINDOW_BEFORE_MS: i64 = 15 * 60 * 1000;
const KICKOFF_WINDOW_AFTER_MS: i64 = 4 * 3_600 * 1_000;

struct LiveFixture {
    fixture_id: u64,
    home: String,
    away: String,
}

/// Spawn the autonomous polling loop. Fire-and-forget: a single poll
/// failure (network hiccup, TxLINE error) is logged and the loop waits for
/// the next cycle rather than tearing down — matching `spawn_loopback`'s own
/// "diagnostics failure shouldn't sink the app" posture.
pub fn spawn(app: AppHandle) -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        loop {
            let poll_secs = {
                let state = app.state::<DesktopState>();
                state.config.autonomous_poll_secs.max(5)
            };
            tokio::time::sleep(Duration::from_secs(poll_secs)).await;

            let state = app.state::<DesktopState>();
            if !state.autonomous_enabled.load(Ordering::Relaxed) {
                continue;
            }

            if let Err(err) = poll_once(&app, &state).await {
                eprintln!("autonomous loop: poll cycle failed, retrying next cycle: {err}");
            }
        }
    })
}

async fn poll_once(app: &AppHandle, state: &DesktopState) -> Result<(), AppError> {
    let fixtures = fetch_live_window_fixtures(state).await?;
    for fixture in fixtures {
        if let Err(err) = poll_fixture(app, state, &fixture).await {
            eprintln!(
                "autonomous loop: fixture {} poll failed: {err}",
                fixture.fixture_id
            );
        }
    }
    Ok(())
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

async fn fetch_live_window_fixtures(state: &DesktopState) -> Result<Vec<LiveFixture>, AppError> {
    let epoch_day = now_ms() / 86_400_000;
    let raw = authenticated_get(
        &state.client,
        &state.config,
        "api/fixtures/snapshot",
        vec![("startEpochDay", epoch_day.to_string())],
    )
    .await?;
    Ok(parse_live_fixtures(&raw, now_ms()))
}

fn parse_live_fixtures(raw: &Value, now_ms: i64) -> Vec<LiveFixture> {
    let Some(rows) = raw.as_array() else { return Vec::new() };
    let mut out = Vec::new();
    for row in rows {
        let Some(fixture_id) = row.get("FixtureId").and_then(Value::as_u64) else { continue };
        let Some(start_time) = row.get("StartTime").and_then(Value::as_i64) else { continue };
        let delta = now_ms - start_time;
        if delta < -KICKOFF_WINDOW_BEFORE_MS || delta > KICKOFF_WINDOW_AFTER_MS {
            continue;
        }
        let home = row.get("Participant1").and_then(Value::as_str).unwrap_or("Home").to_string();
        let away = row.get("Participant2").and_then(Value::as_str).unwrap_or("Away").to_string();
        out.push(LiveFixture { fixture_id, home, away });
    }
    out
}

/// Keep only the latest tick per outcome — a live snapshot can carry more
/// than one row per market (different bookmakers/timestamps).
fn latest_per_outcome(ticks: &[OddsTick]) -> HashMap<&'static str, (i64, f64)> {
    let mut latest = HashMap::new();
    for tick in ticks {
        latest
            .entry(tick.outcome)
            .and_modify(|(ts, decimal): &mut (i64, f64)| {
                if tick.ts > *ts {
                    *ts = tick.ts;
                    *decimal = tick.decimal;
                }
            })
            .or_insert((tick.ts, tick.decimal));
    }
    latest
}

async fn poll_fixture(app: &AppHandle, state: &DesktopState, fixture: &LiveFixture) -> Result<(), AppError> {
    let raw = authenticated_get(
        &state.client,
        &state.config,
        &format!("api/odds/snapshot/{}", fixture.fixture_id),
        vec![],
    )
    .await?;
    let mut ticks = Vec::new();
    parse_market_rows(&raw, fixture.fixture_id, &mut ticks);
    if ticks.is_empty() {
        return Ok(()); // Market not open yet / no 1X2 quotes right now.
    }
    let latest = latest_per_outcome(&ticks);

    // Diff against the last poll. First time this fixture/outcome is seen,
    // there's nothing to compare against — just record the baseline.
    let mut triggered: Option<(&'static str, f64, f64, f64)> = None;
    {
        let mut last_seen = state
            .autonomous_last_seen
            .lock()
            .map_err(|_| AppError::LockPoisoned)?;
        for (&outcome, &(_, current)) in &latest {
            let key = (fixture.fixture_id, outcome.to_string());
            if let Some(&previous) = last_seen.get(&key) {
                if previous > 0.0 {
                    let pct = ((current - previous) / previous * 100.0).abs();
                    if pct >= state.config.odds_move_trigger_pct && triggered.is_none() {
                        triggered = Some((outcome, previous, current, pct));
                    }
                }
            }
            last_seen.insert(key, current);
        }
    }

    let Some((outcome, previous, current, pct)) = triggered else { return Ok(()) };

    // Rate limit: one autonomously-triggered round per fixture per hour.
    {
        let mut last_triggered = state
            .autonomous_last_triggered
            .lock()
            .map_err(|_| AppError::LockPoisoned)?;
        if let Some(&last) = last_triggered.get(&fixture.fixture_id) {
            if last.elapsed() < MIN_RETRIGGER_INTERVAL {
                return Ok(());
            }
        }
        last_triggered.insert(fixture.fixture_id, Instant::now());
    }

    eprintln!(
        "autonomous loop: fixture {} sharp move on {outcome} ({previous:.2} -> {current:.2}, {pct:.1}%), triggering round",
        fixture.fixture_id
    );

    let odds_quotes: Vec<OddsQuote> = latest
        .iter()
        .map(|(&name, &(ts, decimal))| OddsQuote {
            fixture_id: fixture.fixture_id,
            outcome: name.to_string(),
            decimal,
            implied_probability: if decimal > 0.0 { 1.0 / decimal } else { 0.0 },
            source: Some("autonomous-loop".to_string()),
            ts: ts_to_iso(ts),
        })
        .collect();

    let event = TxLineEvent {
        id: format!("autonomous-{}-{}", fixture.fixture_id, now_iso()),
        kind: TxLineEventKind::OddsMove,
        fixture_id: fixture.fixture_id,
        seq: None,
        txline_ts: None,
        action: None,
        confirmed: None,
        participant: None,
        period: None,
        stat_keys: vec!["odds.autonomous".to_string()],
        schema_family: Some("odds".to_string()),
        title: format!("{} vs {}", fixture.home, fixture.away),
        body: format!(
            "Autonomous trigger: {outcome} odds moved {pct:.1}% ({previous:.2} \u{2192} {current:.2})"
        ),
        ts: now_iso(),
        raw: None,
        odds: Some(odds_quotes),
        score: None,
        proof: None,
    };

    run_match_intelligence_round(app.clone(), state, event, TrackMode::Trading).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_live_fixtures_keeps_only_fixtures_in_kickoff_window() {
        let now = 1_000_000_000_000_i64;
        let raw: Value = serde_json::from_str(&format!(
            r#"[
                {{"FixtureId":1,"Participant1":"A","Participant2":"B","StartTime":{}}},
                {{"FixtureId":2,"Participant1":"C","Participant2":"D","StartTime":{}}},
                {{"FixtureId":3,"Participant1":"E","Participant2":"F","StartTime":{}}}
            ]"#,
            now - 3_600_000,           // 1h ago — in window
            now - 5 * 3_600_000,       // 5h ago — outside the 4h post-kickoff window
            now + 30 * 60_000,         // 30 min from now — outside the 15 min pre-kickoff window
        ))
        .unwrap();
        let fixtures = parse_live_fixtures(&raw, now);
        assert_eq!(fixtures.len(), 1);
        assert_eq!(fixtures[0].fixture_id, 1);
    }

    #[test]
    fn parse_live_fixtures_includes_fixture_about_to_kick_off() {
        let now = 1_000_000_000_000_i64;
        let raw: Value = serde_json::from_str(&format!(
            r#"[{{"FixtureId":9,"Participant1":"A","Participant2":"B","StartTime":{}}}]"#,
            now + 5 * 60_000, // kicks off in 5 minutes
        ))
        .unwrap();
        let fixtures = parse_live_fixtures(&raw, now);
        assert_eq!(fixtures.len(), 1);
    }

    #[test]
    fn parse_live_fixtures_skips_rows_missing_required_fields() {
        let raw: Value = serde_json::from_str(r#"[{"FixtureId":1}, {"StartTime":1000}]"#).unwrap();
        assert!(parse_live_fixtures(&raw, 1_000_000).is_empty());
    }

    #[test]
    fn latest_per_outcome_keeps_the_newest_tick() {
        let ticks = vec![
            OddsTick { ts: 100, outcome: "home", decimal: 2.0 },
            OddsTick { ts: 200, outcome: "home", decimal: 1.9 },
            OddsTick { ts: 150, outcome: "away", decimal: 3.5 },
        ];
        let latest = latest_per_outcome(&ticks);
        assert_eq!(latest.get("home"), Some(&(200, 1.9)));
        assert_eq!(latest.get("away"), Some(&(150, 3.5)));
    }
}
