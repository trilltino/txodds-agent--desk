//! One poll cycle: fetch live fixtures/odds, hand moved pairs to the Venice
//! reasoning agent, and log any signal it confirms as sharp.

use std::collections::HashMap;

use agent_core::{
    error::AgentError,
    safety::{wrap_untrusted, BudgetGuard},
    tools::IdempotencyKey,
};
use tracing::{info, warn};
use uuid::Uuid;

use crate::config::Config;
use crate::signal::{append_signal, unix_now, unix_now_iso, update_open_signals, OddsDirection, SignalRecord};
use crate::txline::{fetch_live_fixtures, fetch_odds};
use crate::venice::{assess_movement, VeniceAgent};

pub(crate) async fn detector_step(
    client: &reqwest::Client,
    config: &Config,
    budget: &BudgetGuard,
    prev_odds: &mut HashMap<(u64, String, String), f64>,
    open_signals: &mut HashMap<String, SignalRecord>,
    seen: &mut std::collections::HashSet<String>,
    reasoning_agent: &VeniceAgent,
) -> Result<usize, AgentError> {
    // Tool call 1: fetch live fixtures.
    budget.record_tool_call();
    let fixtures = fetch_live_fixtures(client, &config.api_base).await?;

    if fixtures.is_empty() {
        return Ok(0);
    }

    let mut new_signals = 0usize;

    for fixture in &fixtures {
        // Tool call 2+: fetch odds per fixture.
        budget.record_tool_call();
        let snapshot = fetch_odds(client, &config.api_base, fixture.id).await?;

        for market in &snapshot.markets {
            for sel in &market.selections {
                let key = (fixture.id, market.key.clone(), sel.name.clone());

                let Some(prev_val) = prev_odds.get(&key).copied() else {
                    prev_odds.insert(key, sel.odds);
                    continue;
                };

                // Only hand this pair to the reasoning agent if the odds
                // actually moved at all. This is NOT a re-introduction of the
                // sharpness threshold — it's "is there anything to look at
                // here" — the LLM still decides sharpness itself via the
                // compute_sharp_movement tool.
                if (sel.odds - prev_val).abs() <= f64::EPSILON {
                    continue;
                }

                // Epoch bucket: round to nearest 5 minutes so signals in the
                // same window share an idempotency key.
                let epoch_bucket = unix_now() / 300;
                let idem_raw = format!(
                    "smd:{}:{}:{}:{}",
                    fixture.id, market.key, sel.name, epoch_bucket
                );
                let idempotency_key = IdempotencyKey::new_for(&idem_raw);
                let idem_str = idempotency_key.to_string();

                // Skip if already logged in this or a previous run.
                if seen.contains(&idem_str) {
                    prev_odds.insert(key, sel.odds);
                    continue;
                }

                // ── Venice agent reasoning pass (rig-venice ROADMAP.md Phase 1) ──
                //
                // ALL external data is wrapped in untrusted delimiters before
                // constructing the prompt (§28). The agent decides whether to
                // call compute_sharp_movement, fetch_odds_snapshot, or
                // fetch_active_fixtures, and produces a free-text rationale.
                // Only the deterministic compute_sharp_movement tool result
                // gates whether a signal is recorded — never the model's text.
                let assessment = assess_movement(
                    reasoning_agent,
                    budget,
                    config.max_tool_rounds,
                    fixture,
                    &market.key,
                    &sel.name,
                    sel.odds,
                    prev_val,
                )
                .await;

                let Some((movement, narrative)) = assessment else {
                    prev_odds.insert(key, sel.odds);
                    continue;
                };

                if !movement.is_sharp_move || movement.confidence < config.confidence_gate {
                    prev_odds.insert(key, sel.odds);
                    continue;
                }

                let Some(direction) = OddsDirection::from_tool_str(&movement.direction) else {
                    warn!(direction = %movement.direction, "unrecognised direction from compute_sharp_movement tool");
                    prev_odds.insert(key, sel.odds);
                    continue;
                };

                let signal = SignalRecord {
                    idempotency_key: idem_str.clone(),
                    signal_id: Uuid::new_v4().to_string(),
                    fixture_id: fixture.id,
                    fixture_name: fixture.name.clone(),
                    market_key: market.key.clone(),
                    selection: sel.name.clone(),
                    odds_now: sel.odds,
                    odds_prev: prev_val,
                    move_pct: movement.pct_change,
                    direction,
                    confidence: movement.confidence,
                    detected_at: unix_now_iso(),
                    narrative,
                    correct_so_far: false,
                    outcome: None,
                };

                // Append to JSONL log (tamper-evident audit, §24, §38).
                append_signal(&config.signal_log_path, &signal)?;
                seen.insert(idem_str);
                open_signals.insert(signal.signal_id.clone(), signal.clone());

                info!(
                    fixture = fixture.id,
                    market = %market.key,
                    selection = %wrap_untrusted("selection", &sel.name),
                    odds_now = sel.odds,
                    odds_prev = prev_val,
                    move_pct = movement.pct_change,
                    confidence = movement.confidence,
                    "sharp signal detected"
                );

                new_signals += 1;
                prev_odds.insert(key, sel.odds);
            }
        }
    }

    // ── Prediction tracking: update open signals ──────────────────────────────
    update_open_signals(open_signals, prev_odds, &config.signal_log_path);

    Ok(new_signals)
}
