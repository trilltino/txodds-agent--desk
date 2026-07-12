//! Fan-pundit narrative reaction — rig-venice ROADMAP.md Phase 5.
//!
//! Reacts to a wager `wager_agent.rs` already proposed and the Authority
//! already adjudicated: endorses (nudges `model_prob` up) or challenges
//! (nudges down, or overrides to `NoBet` if severe) the proposal, then hands
//! the nudged wager back through `authority::adjudicate` — the Authority
//! re-derives edge/stake from the nudged probability exactly as it did the
//! first time. The LLM never sets the nudge magnitude, the stake, or the
//! final status directly; those stay deterministic, mirroring
//! `sharp-movement-detector`'s "trust the tool/policy, not the model's
//! self-reported numbers" invariant.
//!
//! ## Honest limitation
//!
//! There is no live narrative/news/injury feed anywhere in this codebase —
//! same limitation as `wager_agent.rs`'s missing fundamentals feed. So this
//! agent reacts to the wager's own thesis and edge, not external news. This
//! is a real second opinion (an independent Venice call reasoning
//! adversarially about a sibling agent's proposal) but it is not the Python
//! `fan-pundit-agent` design's "reads actual injury/news narrative" ideal —
//! don't claim otherwise.

use agent_core::ToolTrailEntry;
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use txodds_types::WagerStatus;

use crate::services::coralos::protocol::FAN_PUNDIT_AGENT;

use super::authority::{self, AuthorityPolicy, AuthorityRuling};

/// Fixed nudge magnitude — mirrors `PUNDIT_CONF_NUDGE` in the Python agent.
/// The LLM picks a stance; it never picks this number.
const CONFIDENCE_NUDGE: f64 = 0.03;

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
struct PunditVerdict {
    /// "endorse" | "challenge" | "no_bet"
    stance: String,
    /// One-sentence justification.
    reason: String,
}

#[derive(Debug)]
struct PunditToolError(String);

impl std::fmt::Display for PunditToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "submit_pundit_verdict error: {}", self.0)
    }
}
impl std::error::Error for PunditToolError {}

#[derive(Debug, Clone, Copy, Default)]
struct SubmitPunditVerdict;

impl Tool for SubmitPunditVerdict {
    const NAME: &'static str = "submit_pundit_verdict";

    type Error = PunditToolError;
    type Args = PunditVerdict;
    type Output = serde_json::Value;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        serde_json::to_value(schemars::schema_for!(PunditVerdict))
            .map(|parameters| ToolDefinition {
                name: Self::NAME.to_owned(),
                description: "Submit your final verdict on the proposed wager and end the \
                    reasoning session. stance must be exactly one of \"endorse\", \"challenge\", \
                    or \"no_bet\" (use no_bet only for a severe objection, not a mild one). \
                    Always include a one-sentence reason."
                    .to_owned(),
                parameters,
            })
            .unwrap_or_else(|e| ToolDefinition {
                name: Self::NAME.to_owned(),
                description: format!("submit_pundit_verdict (schema unavailable: {e})"),
                parameters: serde_json::json!({}),
            })
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        serde_json::to_value(&args).map_err(|e| PunditToolError(e.to_string()))
    }
}

const SYSTEM_PREAMBLE: &str = "\
You are a sports-betting narrative specialist. You are given a wager another \
agent has already proposed — its selection, model probability, market-implied \
probability, edge, and thesis. Your job is to play devil's advocate: does the \
thesis hold up, or is there an obvious reason to doubt it (e.g. the model \
probability rests on neutral/default fundamentals rather than real data, or \
the edge is small relative to how little is actually known)? \
\n\n\
You do not have access to live news or injury reports — reason only from what \
you were given. You MUST call `submit_pundit_verdict` exactly once with your \
stance and a one-sentence reason. Do not respond in plain text.";

/// Outcome of a pundit reaction pass.
pub struct PunditOutcome {
    /// `Some` when the pundit's nudge was re-adjudicated through the
    /// Authority. `None` when there was nothing to react to or Venice
    /// wasn't available — the original ruling stands unchanged in that case.
    pub updated_ruling: Option<AuthorityRuling>,
    pub narrative: String,
    /// Every tool call the Venice loop actually made, attributed to the
    /// fan-pundit persona this pass narrates as (TODO 6e). Empty when the
    /// loop never ran.
    pub tool_trail: Vec<ToolTrailEntry>,
}

/// React to an already-adjudicated wager ruling. Returns `None` for
/// `updated_ruling` (leaving the original ruling as the last word) when:
/// the ruling was already `NoBet` (nothing to endorse or challenge), Venice
/// isn't configured, or the Venice call fails — all non-fatal.
pub async fn react_to_wager(ruling: &AuthorityRuling, policy: AuthorityPolicy) -> PunditOutcome {
    if matches!(ruling.wager.status, WagerStatus::NoBet) {
        return PunditOutcome {
            updated_ruling: None,
            narrative: "no live proposal to react to; holding narrative view".to_owned(),
            tool_trail: Vec::new(),
        };
    }

    let Ok(rig_client) = rig_venice::client() else {
        return PunditOutcome {
            updated_ruling: None,
            narrative: "Venice not configured; pundit reaction skipped".to_owned(),
            tool_trail: Vec::new(),
        };
    };
    let model = rig_venice::model_name();
    let agent = rig_client
        .agent(&model)
        .preamble(SYSTEM_PREAMBLE)
        .tool(SubmitPunditVerdict)
        .build();

    let prompt = format!(
        "Selection: {:?}. Model probability: {:.3}. Market-implied probability: {:.3}. Edge: \
         {:.3}. Thesis: {}",
        ruling.wager.selection, ruling.wager.model_prob, ruling.wager.market_implied,
        ruling.wager.edge, ruling.wager.thesis,
    );

    let outcome =
        rig_venice::loop_runner::run_tool_loop(&agent, prompt, SubmitPunditVerdict::NAME, 3, || {})
            .await;

    let Ok(outcome) = outcome else {
        return PunditOutcome {
            updated_ruling: None,
            narrative: "Venice reasoning failed; pundit reaction skipped".to_owned(),
            tool_trail: Vec::new(),
        };
    };
    let tool_trail = ToolTrailEntry::from_calls(FAN_PUNDIT_AGENT, &outcome.tool_calls);
    let Some(final_args) = outcome.final_args else {
        return PunditOutcome {
            updated_ruling: None,
            narrative: "pundit did not reach a verdict within the round cap".to_owned(),
            tool_trail,
        };
    };
    let Ok(verdict) = serde_json::from_value::<PunditVerdict>(final_args) else {
        return PunditOutcome {
            updated_ruling: None,
            narrative: "malformed pundit verdict; reaction skipped".to_owned(),
            tool_trail,
        };
    };

    // Reconstruct the market decimal odds from the implied probability the
    // Authority already recorded (`implied = 1 / decimal`), rather than
    // threading the raw odds through a second parameter.
    let market_decimal_odds = 1.0 / ruling.wager.market_implied;
    if !market_decimal_odds.is_finite() || market_decimal_odds <= 1.0 {
        return PunditOutcome {
            updated_ruling: None,
            narrative: "could not reconstruct market odds; reaction skipped".to_owned(),
            tool_trail,
        };
    }

    let nudge = match verdict.stance.as_str() {
        "endorse" => CONFIDENCE_NUDGE,
        "challenge" | "no_bet" => -CONFIDENCE_NUDGE,
        _ => 0.0,
    };
    let nudged_prob = (ruling.wager.model_prob + nudge).clamp(0.01, 0.99);

    let mut nudged_wager = ruling.wager.clone();
    nudged_wager.model_prob = nudged_prob;
    nudged_wager.thesis = format!(
        "{} | Pundit {}: {}",
        nudged_wager.thesis, verdict.stance, verdict.reason
    );

    let mut updated = authority::adjudicate(nudged_wager, market_decimal_odds, policy);
    if verdict.stance == "no_bet" {
        updated.wager.status = WagerStatus::NoBet;
        updated.wager.stake_sol = 0.0;
        updated.reason = format!("{} [pundit override: severe objection]", updated.reason);
    }

    let narrative = format!(
        "pundit {} ({}): {}",
        verdict.stance, verdict.reason, updated.reason
    );
    PunditOutcome { updated_ruling: Some(updated), narrative, tool_trail }
}

#[cfg(test)]
mod tests {
    use super::*;
    use txodds_types::{Selection, Wager, WagerStatus};

    fn no_bet_ruling() -> AuthorityRuling {
        AuthorityRuling {
            wager: Wager {
                wager_id: "w1".into(),
                fixture_id: 1,
                selection: Selection::Home,
                model_prob: 0.5,
                market_implied: 0.5,
                edge: 0.0,
                fair_odds: 2.0,
                stake_sol: 0.0,
                thesis: "no edge".into(),
                proof_ref: None,
                status: WagerStatus::NoBet,
                debate: None,
                created_at: "2026-01-01T00:00:00.000Z".into(),
            },
            reason: "no edge".into(),
        }
    }

    #[tokio::test]
    async fn skips_when_ruling_already_no_bet() {
        let policy = AuthorityPolicy::from_max_spend(0.05);
        let outcome = react_to_wager(&no_bet_ruling(), policy).await;
        assert!(outcome.updated_ruling.is_none());
        assert!(outcome.narrative.contains("no live proposal"));
    }

    #[tokio::test]
    async fn skips_when_venice_unconfigured() {
        std::env::remove_var("VENICE_API_KEY");
        let ruling = AuthorityRuling {
            wager: Wager {
                wager_id: "w2".into(),
                fixture_id: 1,
                selection: Selection::Home,
                model_prob: 0.6,
                market_implied: 0.5,
                edge: 0.1,
                fair_odds: 1.0 / 0.6,
                stake_sol: 0.01,
                thesis: "value on home".into(),
                proof_ref: None,
                status: WagerStatus::Debated,
                debate: None,
                created_at: "2026-01-01T00:00:00.000Z".into(),
            },
            reason: "edge positive".into(),
        };
        let policy = AuthorityPolicy::from_max_spend(0.05);
        let outcome = react_to_wager(&ruling, policy).await;
        assert!(outcome.updated_ruling.is_none());
        assert!(outcome.narrative.contains("not configured"));
    }
}
