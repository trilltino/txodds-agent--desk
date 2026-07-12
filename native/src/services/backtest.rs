//! Arena backtest replay engine.
//!
//! Walks a completed fixture's real TxLINE odds history hour-by-hour (never
//! the whole-fixture `/api/odds/updates/{fixtureId}` endpoint — verified
//! against the live API at ~27 MB / 75k rows for one match, too heavy to
//! fetch whole; the interval endpoint returns the same data in ~1 MB
//! hourly chunks), detects sharp 1X2 moves with the exact same deterministic
//! tool the live LLM-driven agents use (`rig_venice::tools::ComputeSharpMovement`
//! — no LLM call, it's pure computation), and settles simulated FollowSharp /
//! FadeSharp positions against the fixture's real final score.
//!
//! Selection/direction convention mirrors the real, already-running
//! `contrarian`/`match-intelligence` agents exactly (see
//! `crates/agents/contrarian/src/main.rs`): both agents take a position on
//! the *same* selection, never opposite selections — only `direction` (With
//! / Against) flips between them. Settlement math is the same formula used
//! by `crates/agents/arena-coordinator/src/main.rs`: `pnl = odds - 1.0` if
//! won, else `-1.0`; `won = selection_won` for `With`, `!selection_won` for
//! `Against`.
//!
//! ARENA-AUTONOMY-PLAN.md Priority B.

use agent_core::arena::{AgentLeaderboardEntry, ArenaPosition, PositionDirection, PositionOutcome, Strategy};
use reqwest::Client;
use rig_venice::tools::{ComputeMovementInput, ComputeSharpMovement};
use rig::tool::Tool;
use serde::Serialize;
use serde_json::Value;

use crate::config::AppConfig;
use crate::error::AppError;
use crate::services::txline::api::authenticated_get;
use crate::types::now_iso;

const DAY_MS: i64 = 86_400_000;
const HOUR_MS: i64 = 3_600_000;
/// Hours before kickoff to start fetching odds — covers pre-match line
/// movement without wastefully walking the whole prior day.
const HOURS_BEFORE_KICKOFF: i64 = 1;
/// Hours after kickoff to fetch — covers 90 minutes + stoppage + extra time
/// for matches that go there, plus a safety margin. A live check against a
/// real fixture that went to extra time found odds data still updating at
/// kickoff+3h29m (the actual final score landed a little after that) — 3h
/// alone clipped it, hence the margin here.
const HOURS_AFTER_KICKOFF: i64 = 4;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StrategyTally {
    pub agent_id: String,
    pub positions_taken: u32,
    pub positions_won: u32,
    pub total_pnl_points: f64,
    pub win_rate: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BacktestSummary {
    pub fixture_id: u64,
    pub home: String,
    pub away: String,
    pub final_score: String,
    pub odds_ticks_processed: usize,
    pub signals_detected: usize,
    pub follow: StrategyTally,
    pub fade: StrategyTally,
    /// Every settled position, for detail rendering / persistence. Always
    /// has `outcome` populated — the replay only ever returns positions it
    /// has already settled against the fetched final score.
    pub positions: Vec<ArenaPosition>,
}

/// One parsed 1X2 odds tick: a single outcome's decimal price at a moment.
struct OddsTick {
    ts: i64,
    outcome: &'static str,
    decimal: f64,
}

/// Replay one completed fixture's real odds history and settle simulated
/// FollowSharp/FadeSharp positions against its real final score.
///
/// `home`/`away`/`kickoff_ts_ms` come from the caller (the UI already has
/// them from the fixture board) rather than being re-fetched here.
pub async fn replay_fixture(
    client: &Client,
    config: &AppConfig,
    fixture_id: u64,
    home: &str,
    away: &str,
    kickoff_ts_ms: i64,
) -> Result<BacktestSummary, AppError> {
    let (ticks, final_score_pair) = fetch_ticks_and_final_score(client, config, fixture_id, kickoff_ts_ms).await?;
    let (home_score, away_score) = final_score_pair.ok_or_else(|| {
        AppError::InvalidInput(format!(
            "fixture {fixture_id} has no final score yet — backtest needs a completed match"
        ))
    })?;
    let winner = match home_score.cmp(&away_score) {
        std::cmp::Ordering::Greater => "home",
        std::cmp::Ordering::Less => "away",
        std::cmp::Ordering::Equal => "draw",
    };
    let final_score = format!("{home_score}-{away_score}");
    let odds_ticks_processed = ticks.len();

    let mover = ComputeSharpMovement { threshold_pct: config.odds_move_trigger_pct };
    let mut last_seen: std::collections::HashMap<&'static str, f64> = std::collections::HashMap::new();
    let mut positions: Vec<ArenaPosition> = Vec::new();
    let mut signals_detected = 0usize;

    for tick in &ticks {
        let Some(&previous) = last_seen.get(tick.outcome) else {
            last_seen.insert(tick.outcome, tick.decimal);
            continue;
        };
        last_seen.insert(tick.outcome, tick.decimal);
        if (tick.decimal - previous).abs() < f64::EPSILON {
            continue; // Unchanged re-send — not a move.
        }

        let movement = mover
            .call(ComputeMovementInput {
                selection: tick.outcome.to_string(),
                current_odds: tick.decimal,
                previous_odds: previous,
                market_key: "1x2".to_string(),
            })
            .await
            .map_err(|e| AppError::Task(format!("compute_sharp_movement failed: {e}")))?;
        let is_sharp_move = movement["is_sharp_move"].as_bool().unwrap_or(false);
        if !is_sharp_move {
            continue;
        }
        signals_detected += 1;
        let confidence = movement["confidence"].as_f64().unwrap_or(0.5);
        let move_pct = movement["pct_change"].as_f64().unwrap_or(0.0);
        // Shortening (price dropped) → sharp money backing this outcome.
        // Mirrors contrarian's own convention exactly (see module doc).
        let shortened = tick.decimal < previous;
        let (follow_direction, fade_direction) = if shortened {
            (PositionDirection::With, PositionDirection::Against)
        } else {
            (PositionDirection::Against, PositionDirection::With)
        };
        let recorded_at = ts_to_iso(tick.ts);

        positions.push(settle_position(
            "match-intelligence-backtest",
            Strategy::FollowSharp,
            fixture_id,
            tick.outcome,
            tick.decimal,
            move_pct,
            follow_direction,
            confidence,
            &recorded_at,
            winner,
            &final_score,
        ));
        positions.push(settle_position(
            "contrarian-backtest",
            Strategy::FadeSharp,
            fixture_id,
            tick.outcome,
            tick.decimal,
            move_pct,
            fade_direction,
            confidence,
            &recorded_at,
            winner,
            &final_score,
        ));
    }

    let follow = tally("match-intelligence-backtest", Strategy::FollowSharp, &positions);
    let fade = tally("contrarian-backtest", Strategy::FadeSharp, &positions);

    Ok(BacktestSummary {
        fixture_id,
        home: home.to_string(),
        away: away.to_string(),
        final_score,
        odds_ticks_processed,
        signals_detected,
        follow,
        fade,
        positions,
    })
}

#[allow(clippy::too_many_arguments)]
fn settle_position(
    agent_id: &str,
    strategy: Strategy,
    fixture_id: u64,
    selection: &str,
    odds_at_entry: f64,
    odds_move_pct: f64,
    direction: PositionDirection,
    confidence: f64,
    recorded_at: &str,
    winner: &str,
    final_score: &str,
) -> ArenaPosition {
    // Same formula as arena-coordinator's real settlement (see module doc).
    let selection_won = winner.eq_ignore_ascii_case(selection);
    let won = match direction {
        PositionDirection::With => selection_won,
        PositionDirection::Against => !selection_won,
    };
    let pnl_points = if won { odds_at_entry - 1.0 } else { -1.0 };

    ArenaPosition {
        position_id: uuid::Uuid::new_v4().to_string(),
        agent_id: agent_id.to_string(),
        strategy,
        fixture_id,
        market_key: "1x2".to_string(),
        selection: selection.to_string(),
        odds_at_entry,
        odds_move_pct,
        direction,
        confidence,
        recorded_at: recorded_at.to_string(),
        tx_signature: None,
        outcome: Some(PositionOutcome {
            selection_won: won,
            final_score: final_score.to_string(),
            pnl_points,
            settled_at: now_iso(),
            settlement_tx: None,
        }),
    }
}

fn tally(agent_id: &str, strategy: Strategy, positions: &[ArenaPosition]) -> StrategyTally {
    let entry = AgentLeaderboardEntry::from_positions(agent_id.to_string(), strategy, positions);
    StrategyTally {
        agent_id: entry.agent_id,
        positions_taken: entry.positions_taken,
        positions_won: entry.positions_won,
        total_pnl_points: entry.total_pnl_points,
        win_rate: entry.win_rate,
    }
}

fn ts_to_iso(ts_ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ts_ms)
        .map_or_else(now_iso, |dt| dt.to_rfc3339())
}

// ── TxLINE fetch + parse ─────────────────────────────────────────────────────

/// Final score for a completed fixture, read from the last valid nested
/// `Score.ParticipantN.Total.Goals` action entry — mirrors
/// `ui/core/txline/fixtures.ts`'s `parseScoreSnapshot` exactly (that parser
/// is the verified-correct one; this is its Rust twin for the backend).
///
/// Deliberately NOT `/api/scores/updates/{fixtureId}` — verified live that
/// endpoint returns Server-Sent-Event-framed text (`data: {...}\n\n`), not a
/// plain JSON array, unlike its odds counterpart. `/api/scores/updates/{epochDay}/{hourOfDay}/{interval}`
/// (used here, in the same hourly loop as the odds fetch) is plain JSON —
/// verified live at 24 KB / 48 rows for one hour.
fn latest_score_in_window(raw: &Value, fixture_id: u64) -> Option<(i64, i64)> {
    let rows = raw.as_array()?;
    rows.iter()
        .rev()
        .filter(|row| row.get("FixtureId").and_then(Value::as_u64) == Some(fixture_id))
        .find_map(parse_nested_score)
}

fn parse_nested_score(action: &Value) -> Option<(i64, i64)> {
    let score = action.get("Score")?;
    let p1_home = action
        .get("Participant1IsHome")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let p1_goals = score.pointer("/Participant1/Total/Goals")?.as_i64()?;
    let p2_goals = score.pointer("/Participant2/Total/Goals")?.as_i64()?;
    Some(if p1_home { (p1_goals, p2_goals) } else { (p2_goals, p1_goals) })
}

/// Walk hourly interval windows around kickoff, returning every 1X2
/// full-time odds tick (sorted by timestamp) and the latest score seen —
/// which, once the window runs past full time, is the final score.
///
/// Uses `/api/odds|scores/updates/{epochDay}/{hourOfDay}/1` (bounded, ~1 MB
/// per call) rather than the whole-fixture odds endpoint (verified at 27 MB
/// / 75,540 rows for one fixture — too heavy per match across a 104-match
/// tournament) or the whole-fixture scores endpoint (SSE-framed, not JSON).
async fn fetch_ticks_and_final_score(
    client: &Client,
    config: &AppConfig,
    fixture_id: u64,
    kickoff_ts_ms: i64,
) -> Result<(Vec<OddsTick>, Option<(i64, i64)>), AppError> {
    let mut ticks = Vec::new();
    let mut latest_score = None;
    for hour_offset in -HOURS_BEFORE_KICKOFF..=HOURS_AFTER_KICKOFF {
        let window_ms = kickoff_ts_ms + hour_offset * HOUR_MS;
        let epoch_day = window_ms.div_euclid(DAY_MS);
        let hour_of_day = window_ms.rem_euclid(DAY_MS) / HOUR_MS;

        let odds_raw = authenticated_get(
            client,
            config,
            &format!("api/odds/updates/{epoch_day}/{hour_of_day}/1"),
            vec![],
        )
        .await?;
        parse_market_rows(&odds_raw, fixture_id, &mut ticks);

        let scores_raw = authenticated_get(
            client,
            config,
            &format!("api/scores/updates/{epoch_day}/{hour_of_day}/1"),
            vec![],
        )
        .await?;
        if let Some(score) = latest_score_in_window(&scores_raw, fixture_id) {
            latest_score = Some(score);
        }
    }
    ticks.sort_by_key(|t| t.ts);
    Ok((ticks, latest_score))
}

/// Parse one interval response's market rows into flat `OddsTick`s, keeping
/// only this fixture's full-time 1X2 market. Mirrors
/// `ui/core/txline/fixtures.ts`'s `marketRowQuotes` + `isMatchWinnerSet`
/// filtering (milli-odds ÷1000, `part1`/`part2` → `home`/`away`), scoped down
/// to just the primary market since that's all a backtest replay needs.
fn parse_market_rows(raw: &Value, fixture_id: u64, out: &mut Vec<OddsTick>) {
    let Some(rows) = raw.as_array() else { return };
    for row in rows {
        if row.get("FixtureId").and_then(Value::as_u64) != Some(fixture_id) {
            continue;
        }
        // Only the full-time match-winner market — period markets (extra
        // time, first half) use the same PriceNames but aren't what a
        // sharp-movement signal on the main line should react to.
        let period = row.get("MarketPeriod").and_then(Value::as_str);
        if !matches!(period, None | Some("ft")) {
            continue;
        }
        let Some(names) = row.get("PriceNames").and_then(Value::as_array) else { continue };
        let Some(prices) = row.get("Prices").and_then(Value::as_array) else { continue };
        if names.len() != 3 || prices.len() != 3 {
            continue;
        }
        let Some(ts) = row.get("Ts").and_then(Value::as_i64) else { continue };

        for (name_val, price_val) in names.iter().zip(prices.iter()) {
            let Some(raw_name) = name_val.as_str() else { continue };
            let outcome = match raw_name.to_ascii_lowercase().as_str() {
                "part1" | "home" | "1" => "home",
                "draw" | "x" => "draw",
                "part2" | "away" | "2" => "away",
                _ => continue,
            };
            let Some(raw_price) = price_val.as_f64() else { continue };
            let decimal = if raw_price >= 100.0 { raw_price / 1000.0 } else { raw_price };
            if decimal <= 1.0 {
                continue;
            }
            out.push(OddsTick { ts, outcome, decimal });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_nested_score_reads_totals() {
        let action: Value = serde_json::from_str(
            r#"{"Participant1IsHome":true,"Score":{"Participant1":{"Total":{"Goals":2}},"Participant2":{"Total":{"Goals":1}}}}"#,
        )
        .unwrap();
        assert_eq!(parse_nested_score(&action), Some((2, 1)));
    }

    #[test]
    fn parse_nested_score_swaps_when_not_home() {
        let action: Value = serde_json::from_str(
            r#"{"Participant1IsHome":false,"Score":{"Participant1":{"Total":{"Goals":2}},"Participant2":{"Total":{"Goals":1}}}}"#,
        )
        .unwrap();
        assert_eq!(parse_nested_score(&action), Some((1, 2)));
    }

    #[test]
    fn parse_nested_score_none_when_missing() {
        let action: Value = serde_json::from_str(r#"{"Action":"comment"}"#).unwrap();
        assert_eq!(parse_nested_score(&action), None);
    }

    #[test]
    fn latest_score_in_window_finds_last_matching_row_with_a_score() {
        let raw: Value = serde_json::from_str(
            r#"[
                {"FixtureId":42,"Participant1IsHome":true,"Score":{"Participant1":{"Total":{"Goals":0}},"Participant2":{"Total":{"Goals":0}}}},
                {"FixtureId":99,"Action":"other fixture, ignored"},
                {"FixtureId":42,"Action":"safe_possession"},
                {"FixtureId":42,"Participant1IsHome":true,"Score":{"Participant1":{"Total":{"Goals":1}},"Participant2":{"Total":{"Goals":2}}}}
            ]"#,
        )
        .unwrap();
        assert_eq!(latest_score_in_window(&raw, 42), Some((1, 2)));
    }

    #[test]
    fn latest_score_in_window_none_when_no_row_has_a_score() {
        let raw: Value = serde_json::from_str(r#"[{"FixtureId":42,"Action":"comment"}]"#).unwrap();
        assert_eq!(latest_score_in_window(&raw, 42), None);
    }

    #[test]
    fn parse_market_rows_normalizes_part1_part2_and_milli_odds() {
        let raw: Value = serde_json::from_str(
            r#"[{"FixtureId":42,"Ts":1000,"MarketPeriod":null,"PriceNames":["part1","draw","part2"],"Prices":[2600,3400,2750]}]"#,
        )
        .unwrap();
        let mut out = Vec::new();
        parse_market_rows(&raw, 42, &mut out);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].outcome, "home");
        assert!((out[0].decimal - 2.6).abs() < 1e-9);
        assert_eq!(out[1].outcome, "draw");
        assert_eq!(out[2].outcome, "away");
    }

    #[test]
    fn parse_market_rows_skips_other_fixtures_and_period_markets() {
        let raw: Value = serde_json::from_str(
            r#"[
                {"FixtureId":99,"Ts":1000,"MarketPeriod":null,"PriceNames":["part1","draw","part2"],"Prices":[2600,3400,2750]},
                {"FixtureId":42,"Ts":1000,"MarketPeriod":"et","PriceNames":["part1","draw","part2"],"Prices":[2600,3400,2750]}
            ]"#,
        )
        .unwrap();
        let mut out = Vec::new();
        parse_market_rows(&raw, 42, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn settle_position_with_direction_wins_when_selection_wins() {
        let pos = settle_position(
            "a", Strategy::FollowSharp, 1, "home", 2.0, 5.0, PositionDirection::With, 0.8,
            "2026-01-01T00:00:00Z", "home", "2-1",
        );
        let outcome = pos.outcome.unwrap();
        assert!(outcome.selection_won);
        assert!((outcome.pnl_points - 1.0).abs() < 1e-9);
    }

    #[test]
    fn settle_position_against_direction_wins_when_selection_loses() {
        let pos = settle_position(
            "a", Strategy::FadeSharp, 1, "home", 2.0, 5.0, PositionDirection::Against, 0.8,
            "2026-01-01T00:00:00Z", "away", "0-1",
        );
        let outcome = pos.outcome.unwrap();
        assert!(outcome.selection_won);
        assert!((outcome.pnl_points - 1.0).abs() < 1e-9);
    }

    #[test]
    fn settle_position_loss_is_minus_one() {
        let pos = settle_position(
            "a", Strategy::FollowSharp, 1, "home", 2.0, 5.0, PositionDirection::With, 0.8,
            "2026-01-01T00:00:00Z", "away", "0-1",
        );
        let outcome = pos.outcome.unwrap();
        assert!(!outcome.selection_won);
        assert!((outcome.pnl_points + 1.0).abs() < 1e-9);
    }

    // ── Live E2E check ───────────────────────────────────────────────────────
    //
    // Ignored by default (needs real .env TxLINE credentials + network) —
    // run explicitly with `cargo test -p txodds-agent-desk --lib backtest::tests::live_replay_against_real_txline -- --ignored --nocapture`.
    // Verifies the whole pipeline end-to-end against a real completed
    // fixture, not just each piece in isolation.
    #[tokio::test]
    #[ignore]
    async fn live_replay_against_real_txline() {
        let config = AppConfig::load();
        let client = reqwest::Client::new();
        // Norway vs England, fixture 18213979 — used throughout this
        // session's manual verification; a completed extra-time match.
        let kickoff_ms = 1_783_803_600_000_i64;
        let summary = replay_fixture(&client, &config, 18_213_979, "Norway", "England", kickoff_ms)
            .await
            .expect("live backtest replay failed");

        println!("=== live backtest summary ===");
        println!("final_score={}", summary.final_score);
        println!("odds_ticks_processed={}", summary.odds_ticks_processed);
        println!("signals_detected={}", summary.signals_detected);
        println!(
            "follow: taken={} won={} pnl={:.2}",
            summary.follow.positions_taken, summary.follow.positions_won, summary.follow.total_pnl_points
        );
        println!(
            "fade:   taken={} won={} pnl={:.2}",
            summary.fade.positions_taken, summary.fade.positions_won, summary.fade.total_pnl_points
        );

        // No independently-verified ground truth for this fixture's final
        // score to assert against — just sanity-check the shape (one dash,
        // two non-negative integers) rather than a specific score.
        assert!(
            regex_like_score(&summary.final_score),
            "final_score {} doesn't look like a score",
            summary.final_score
        );
        assert!(summary.odds_ticks_processed > 0, "should have fetched real odds ticks");
        assert!(summary.signals_detected > 0, "should have detected at least one sharp move");
        // Follow and Fade always take equal, opposite-direction positions on
        // the same signals — so exactly one of each pair wins.
        assert_eq!(summary.follow.positions_taken, summary.fade.positions_taken);
        assert_eq!(
            summary.follow.positions_won + summary.fade.positions_won,
            summary.follow.positions_taken,
            "exactly one side of each With/Against pair should win — wins should sum to the pair count"
        );
        // NOT asserting follow.pnl == -fade.pnl: a win pays (odds - 1) but a
        // loss always costs exactly -1, so PnL is only symmetric when the
        // entry odds happen to be 2.0. Win/loss counts are what's symmetric.

        // ── Persistence round-trip ──────────────────────────────────────────
        //
        // Exercises the other half of `commands::backtest::run_backtest`
        // that `replay_fixture` alone doesn't cover: the Tauri command wraps
        // this same summary in `ledger.replace_backtest_settlements(...)`.
        // Real temp SQLite file, real replay output — not a mock.
        let tmp_path = std::env::temp_dir().join(format!("backtest-e2e-test-{}.sqlite", uuid::Uuid::new_v4()));
        let mut ledger = crate::services::ledger::LedgerStore::open(&tmp_path).expect("open temp ledger");
        ledger
            .replace_backtest_settlements(summary.fixture_id as i64, &summary.home, &summary.away, &summary.positions)
            .expect("persist backtest settlements");

        let persisted = ledger
            .list_backtest_settlements(Some(summary.fixture_id as i64))
            .expect("list backtest settlements");
        assert_eq!(
            persisted.len(),
            summary.positions.len(),
            "every settled position should round-trip through the ledger"
        );
        assert!(persisted.iter().all(|row| row.fixture_home == summary.home && row.fixture_away == summary.away));
        assert!(persisted.iter().any(|row| row.agent_id == "match-intelligence-backtest"));
        assert!(persisted.iter().any(|row| row.agent_id == "contrarian-backtest"));

        // Re-running the replay must REPLACE, not accumulate, per fixture.
        ledger
            .replace_backtest_settlements(summary.fixture_id as i64, &summary.home, &summary.away, &summary.positions)
            .expect("second persist should replace, not duplicate");
        let persisted_again = ledger
            .list_backtest_settlements(Some(summary.fixture_id as i64))
            .expect("list backtest settlements after replace");
        assert_eq!(persisted_again.len(), summary.positions.len(), "replace should not duplicate rows");

        drop(ledger);
        let _ = std::fs::remove_file(&tmp_path);
    }

    fn regex_like_score(s: &str) -> bool {
        let Some((home, away)) = s.split_once('-') else { return false };
        home.parse::<u32>().is_ok() && away.parse::<u32>().is_ok()
    }
}
