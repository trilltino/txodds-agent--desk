//! Fundamentals wager proposal — rig-venice ROADMAP.md Phase 4.
//!
//! Wires `super::authority::adjudicate` — the deterministic Rust Authority,
//! built and tested but never called from the live pipeline before this —
//! into `run_match_intelligence_round` for the first time. A Venice agent
//! reasons over the fundamentals softmax model and the market odds actually
//! on offer for this fixture, and may propose a wager; the Authority
//! re-derives edge, sizes with Kelly, and clamps before anything downstream
//! trusts it. The LLM never sizes stake, never bypasses the proof gate, and
//! never decides the final wager status — see rig-venice ROADMAP.md's
//! non-negotiable invariant.
//!
//! ## Honest limitation
//!
//! `ComputeModelProbability`'s inputs (form, xG, rank, injuries, h2h) have
//! **no live data source in this app**. TxLINE supplies odds and score
//! events, not pre-match team-fundamentals stats — there is nowhere in this
//! codebase that fetches form/xG/injury data for a fixture. So today this
//! always runs the softmax model on neutral defaults, which only ever
//! yields the home-advantage-only baseline distribution. That is still a
//! real (if simple) comparison against the market's fair-stripped
//! probability — not a fabricated data source — but it is not the
//! fully-informed fundamentals model the Python `coral-agents` design
//! describes. Wiring a real fundamentals feed is future work, not something
//! to fake here.

use agent_core::ToolTrailEntry;
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use txodds_types::{Selection, TxLineEvent, Wager, WagerStatus};

use crate::services::coralos::protocol::MATCH_INTELLIGENCE_AGENT;

use super::authority::{self, AuthorityPolicy, AuthorityRuling};

// ── Odds extraction ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Default)]
struct OddsBySelection {
    home: Option<f64>,
    draw: Option<f64>,
    away: Option<f64>,
}

/// Pull decimal odds per 1X2 outcome from the event's odds quotes, matching
/// the same alias set the Python `coral-agents` fundamentals agent uses
/// (`home`/`HOME`/`1`, `draw`/`DRAW`/`X`, `away`/`AWAY`/`2`), case-insensitive.
fn extract_odds(event: &TxLineEvent) -> OddsBySelection {
    let mut out = OddsBySelection::default();
    let Some(quotes) = &event.odds else {
        return out;
    };
    for quote in quotes {
        match quote.outcome.to_ascii_lowercase().as_str() {
            "home" | "1" | "h" => out.home.get_or_insert(quote.decimal),
            "draw" | "x" | "d" => out.draw.get_or_insert(quote.decimal),
            "away" | "2" | "a" => out.away.get_or_insert(quote.decimal),
            _ => continue,
        };
    }
    out
}

// ── Forced structured output tool ─────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
struct WagerAssessment {
    /// True only if the fundamentals model meaningfully diverges from the
    /// market's fair-stripped probability for some selection.
    has_value: bool,
    /// Which selection carries the value: "home" | "draw" | "away". Required
    /// when `has_value` is true, ignored otherwise.
    #[serde(default)]
    selection: Option<String>,
    /// One-sentence explanation of the assessment.
    thesis: String,
}

#[derive(Debug)]
struct WagerToolError(String);

impl std::fmt::Display for WagerToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "submit_wager_assessment error: {}", self.0)
    }
}
impl std::error::Error for WagerToolError {}

/// Forces the agent's final answer into a structured shape. Calling this
/// tool is how the agent signals "I'm done reasoning" — mirrors
/// `SubmitSignalDecision` in `sharp-movement-detector`.
#[derive(Debug, Clone, Copy, Default)]
struct SubmitWagerAssessment;

impl Tool for SubmitWagerAssessment {
    const NAME: &'static str = "submit_wager_assessment";

    type Error = WagerToolError;
    type Args = WagerAssessment;
    type Output = serde_json::Value;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        serde_json::to_value(schemars::schema_for!(WagerAssessment))
            .map(|parameters| ToolDefinition {
                name: Self::NAME.to_owned(),
                description: "Submit your final wager assessment and end the reasoning \
                    session. Call this exactly once, after calling compute_model_probability \
                    and compute_fair_probability, with your conclusion and a one-sentence \
                    thesis. Do not call any other tool afterward."
                    .to_owned(),
                parameters,
            })
            .unwrap_or_else(|e| ToolDefinition {
                name: Self::NAME.to_owned(),
                description: format!("submit_wager_assessment (schema unavailable: {e})"),
                parameters: serde_json::json!({}),
            })
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        serde_json::to_value(&args).map_err(|e| WagerToolError(e.to_string()))
    }
}

const SYSTEM_PREAMBLE: &str = "\
You are a sports-betting fundamentals analyst. You are given a fixture's \
market odds for the 1X2 (home/draw/away) market and told whether team-level \
fundamentals data (form, xG, rank, injuries, head-to-head) is available. \
\n\n\
You MUST call `compute_model_probability` to get an objective fair \
distribution — never estimate it yourself. If told fundamentals are \
unavailable, call it with all-neutral inputs (this yields the \
home-advantage-only baseline, which is still a legitimate comparison point, \
not a placeholder to skip). You MUST also call `compute_fair_probability` \
with the market odds you were given to get the overround-stripped market \
view. \
\n\n\
Once you have called both, decide whether either selection's model \
probability diverges meaningfully from the market's fair probability — that \
divergence is the only thing that would make this worth flagging. You MUST \
call `submit_wager_assessment` exactly once with your conclusion. Do not \
respond in plain text and do not call any tool after \
submit_wager_assessment.";

/// Outcome of a wager-proposal reasoning pass.
pub struct WagerProposalOutcome {
    /// `Some` only when the agent proposed a wager AND the Authority
    /// adjudicated it (regardless of whether the ruling is a bet or `NoBet`
    /// — `NoBet` is still a real ruling, not an absence of one).
    pub ruling: Option<AuthorityRuling>,
    /// Human-readable summary for the transcript, always present.
    pub narrative: String,
    /// Every tool call the Venice loop actually made (name + deterministic
    /// result), for the transcript's `toolTrail` payload (TODO 6e). Empty
    /// when the loop never ran (no market, Venice unconfigured/failed).
    pub tool_trail: Vec<ToolTrailEntry>,
}

/// Run the fundamentals reasoning pass for one triggering event and, if the
/// agent proposes a wager, adjudicate it through the Rust Authority.
///
/// Returns a narrative-only outcome (no ruling) when: the event doesn't
/// carry a complete 1X2 market, Venice isn't configured, the Venice call
/// fails, or the agent concludes there's no value — all non-fatal, matching
/// the rest of this codebase's "LLM failures degrade to a skipped step, not
/// a crash" convention.
pub async fn propose_wager(
    event: &TxLineEvent,
    proof_ref: Option<String>,
    policy: AuthorityPolicy,
) -> WagerProposalOutcome {
    let odds = extract_odds(event);
    let (Some(home_odds), Some(draw_odds), Some(away_odds)) = (odds.home, odds.draw, odds.away)
    else {
        return WagerProposalOutcome {
            ruling: None,
            narrative: "incomplete 1X2 market odds on this event; no wager proposed".to_owned(),
            tool_trail: Vec::new(),
        };
    };

    let Ok(rig_client) = rig_venice::client() else {
        return WagerProposalOutcome {
            ruling: None,
            narrative: "Venice not configured; no wager proposed".to_owned(),
            tool_trail: Vec::new(),
        };
    };
    let model = rig_venice::model_name();
    let agent = rig_client
        .agent(&model)
        .preamble(SYSTEM_PREAMBLE)
        .tool(rig_venice::tools::ComputeModelProbability::default())
        .tool(rig_venice::tools::ComputeFairProbability)
        .tool(SubmitWagerAssessment)
        .build();

    let prompt = format!(
        "Fixture {}. Market odds — home: {home_odds:.2}, draw: {draw_odds:.2}, away: \
         {away_odds:.2}. Team-fundamentals data is NOT available for this fixture (no form/xG/\
         rank/injuries/h2h feed exists) — call compute_model_probability with all-neutral inputs.",
        event.fixture_id
    );

    let outcome =
        rig_venice::loop_runner::run_tool_loop(&agent, prompt, SubmitWagerAssessment::NAME, 6, || {})
            .await;

    let Ok(outcome) = outcome else {
        return WagerProposalOutcome {
            ruling: None,
            narrative: "Venice reasoning failed; no wager proposed".to_owned(),
            tool_trail: Vec::new(),
        };
    };
    // The loop ran — from here on every outcome (including the no-wager
    // ones) carries the real trail of what the agent actually did.
    let tool_trail = ToolTrailEntry::from_calls(MATCH_INTELLIGENCE_AGENT, &outcome.tool_calls);
    let Some(final_args) = outcome.final_args.clone() else {
        return WagerProposalOutcome {
            ruling: None,
            narrative: "agent did not reach a wager decision within the round cap".to_owned(),
            tool_trail,
        };
    };
    let Ok(assessment) = serde_json::from_value::<WagerAssessment>(final_args) else {
        return WagerProposalOutcome {
            ruling: None,
            narrative: "malformed wager assessment; no wager proposed".to_owned(),
            tool_trail,
        };
    };

    if !assessment.has_value {
        return WagerProposalOutcome { ruling: None, narrative: assessment.thesis, tool_trail };
    }

    let Some(selection_str) = assessment.selection.as_deref() else {
        return WagerProposalOutcome {
            ruling: None,
            narrative: "agent claimed value but named no selection; no wager proposed".to_owned(),
            tool_trail,
        };
    };
    let (selection, market_odds) = match selection_str.to_ascii_lowercase().as_str() {
        "home" => (Selection::Home, home_odds),
        "draw" => (Selection::Draw, draw_odds),
        "away" => (Selection::Away, away_odds),
        other => {
            return WagerProposalOutcome {
                ruling: None,
                narrative: format!("unrecognised selection '{other}'; no wager proposed"),
                tool_trail,
            }
        }
    };

    // The model probability comes from the deterministic
    // `compute_model_probability` tool result, never from the model's own
    // prose — same "trust the tool, not the narration" invariant as
    // sharp-movement-detector's Phase 1.
    let Some(model_probs) = outcome.tool_result("compute_model_probability") else {
        return WagerProposalOutcome {
            ruling: None,
            narrative: "agent never called compute_model_probability; no wager proposed".to_owned(),
            tool_trail,
        };
    };
    let field = match selection {
        Selection::Home => "home",
        Selection::Draw => "draw",
        Selection::Away => "away",
    };
    let Some(model_prob) = model_probs.get(field).and_then(serde_json::Value::as_f64) else {
        return WagerProposalOutcome {
            ruling: None,
            narrative: "compute_model_probability result missing the chosen selection".to_owned(),
            tool_trail,
        };
    };

    let wager = Wager {
        wager_id: format!("wager-{}", uuid::Uuid::new_v4()),
        fixture_id: event.fixture_id,
        selection,
        model_prob,
        market_implied: 0.0, // recomputed by authority::adjudicate from market_odds
        edge: 0.0,
        fair_odds: 0.0,
        stake_sol: 0.0,
        thesis: assessment.thesis,
        proof_ref,
        status: WagerStatus::Proposed,
        debate: None,
        created_at: crate::types::now_iso(),
    };

    let ruling = authority::adjudicate(wager, market_odds, policy);
    WagerProposalOutcome { narrative: ruling.reason.clone(), ruling: Some(ruling), tool_trail }
}

#[cfg(test)]
mod tests {
    use super::*;
    use txodds_types::{OddsQuote, TxLineEventKind};

    fn quote(outcome: &str, decimal: f64) -> OddsQuote {
        OddsQuote {
            fixture_id: 1,
            outcome: outcome.to_owned(),
            decimal,
            implied_probability: 1.0 / decimal,
            source: None,
            ts: "2026-01-01T00:00:00.000Z".to_owned(),
        }
    }

    fn event_with_odds(quotes: Vec<OddsQuote>) -> TxLineEvent {
        TxLineEvent {
            id: "event".to_owned(),
            kind: TxLineEventKind::OddsUpdate,
            fixture_id: 1,
            seq: Some(1),
            txline_ts: None,
            action: None,
            confirmed: None,
            participant: None,
            period: None,
            stat_keys: vec![],
            schema_family: None,
            title: "title".to_owned(),
            body: "body".to_owned(),
            ts: "2026-01-01T00:00:00.000Z".to_owned(),
            raw: None,
            odds: Some(quotes),
            score: None,
            proof: None,
        }
    }

    #[test]
    fn extract_odds_matches_known_aliases_case_insensitively() {
        let event = event_with_odds(vec![
            quote("HOME", 1.90),
            quote("X", 3.60),
            quote("2", 4.20),
        ]);
        let odds = extract_odds(&event);
        assert_eq!(odds.home, Some(1.90));
        assert_eq!(odds.draw, Some(3.60));
        assert_eq!(odds.away, Some(4.20));
    }

    #[test]
    fn extract_odds_ignores_unknown_outcomes() {
        let event = event_with_odds(vec![quote("total_over_2.5", 1.80)]);
        let odds = extract_odds(&event);
        assert_eq!(odds.home, None);
        assert_eq!(odds.draw, None);
        assert_eq!(odds.away, None);
    }

    #[tokio::test]
    async fn propose_wager_skips_when_market_incomplete() {
        let event = event_with_odds(vec![quote("home", 1.90)]); // missing draw/away
        let policy = AuthorityPolicy::from_max_spend(0.05);
        let outcome = propose_wager(&event, None, policy).await;
        assert!(outcome.ruling.is_none());
        assert!(outcome.narrative.contains("incomplete"));
    }

    #[tokio::test]
    async fn propose_wager_skips_when_venice_unconfigured() {
        // Ensure no stray VENICE_API_KEY from another test/process leaks in.
        std::env::remove_var("VENICE_API_KEY");
        let event = event_with_odds(vec![quote("home", 1.90), quote("draw", 3.60), quote("away", 4.20)]);
        let policy = AuthorityPolicy::from_max_spend(0.05);
        let outcome = propose_wager(&event, None, policy).await;
        assert!(outcome.ruling.is_none());
        assert!(outcome.narrative.contains("not configured"));
    }
}
