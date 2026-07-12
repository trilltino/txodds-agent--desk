//! fan-pundit-agent — real, independent CoralOS narrative-reaction participant.
//!
//! A standalone OS process registered with coral-server (see
//! `crates/agents/fan-pundit-agent/coral-agent.toml`), launched by
//! coral-server itself, that blocks on its own `wait_for_mention` loop
//! (`coral_client::run`) and reacts to wager proposals with an independent
//! Venice LLM call — endorsing, challenging, or vetoing the proposed wager.
//!
//! The orchestrator (`native/`) cannot see this process's reasoning, only
//! the `PUNDIT_REACT_VERDICT` message it publishes back on the Coral thread.
//!
//! Wire grammar (flat `VERB key=value` — same convention as proof-guard-agent):
//!
//!   PUNDIT_REACT_REQUESTED wager=<json>
//!   PUNDIT_REACT_VERDICT   stance=<endorse|challenge|no_bet> reason="..." wager=<json>

use coral_client::{wire, CoralMention, Specialist};
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use txodds_types::wager::Wager;

/// Fixed nudge magnitude — mirrors `PUNDIT_CONF_NUDGE` in the Python agent.
/// The LLM picks a stance; it never picks this number.
const CONFIDENCE_NUDGE: f64 = 0.03;

// ── Venice tool: submit_pundit_verdict ─────────────────────────────────────────

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
You are a sports-betting narrative specialist running as an independent CoralOS \
agent. You are given a wager another agent has already proposed — its selection, \
model probability, market-implied probability, edge, and thesis. Your job is to \
play devil's advocate: does the thesis hold up, or is there an obvious reason to \
doubt it (e.g. the model probability rests on neutral/default fundamentals rather \
than real data, or the edge is small relative to how little is actually known)? \
\n\n\
You do not have access to live news or injury reports — reason only from what \
you were given. You MUST call `submit_pundit_verdict` exactly once with your \
stance and a one-sentence reason. Do not respond in plain text.";

// ── Specialist implementation ─────────────────────────────────────────────────

struct FanPunditSpecialist;

#[async_trait::async_trait]
impl Specialist for FanPunditSpecialist {
    fn name(&self) -> &str {
        "fan-pundit-agent"
    }

    async fn handle(&self, mention: CoralMention) -> String {
        if wire::verb(&mention.text) != "PUNDIT_REACT_REQUESTED" {
            tracing::debug!(text = %mention.text, "fan-pundit-agent: ignoring non-delegation mention");
            return String::new();
        }

        let Some(wager) = parse_wager(&mention.text) else {
            tracing::warn!(text = %mention.text, "fan-pundit-agent: missing/malformed wager= payload");
            return "PUNDIT_REACT_VERDICT stance=challenge reason=\"malformed delegation: no wager payload\" wager={}".to_owned();
        };

        // If already NoBet, nothing to react to.
        if matches!(wager.status, txodds_types::wager::WagerStatus::NoBet) {
            let wager_json = serde_json::to_string(&wager).unwrap_or_default();
            return format!(
                "PUNDIT_REACT_VERDICT stance=endorse reason=\"no live proposal to react to\" wager={wager_json}"
            );
        }

        // Run Venice reasoning loop to get a verdict.
        let verdict = run_venice_verdict(&wager).await;

        // Apply the nudge to model_prob, re-derive edge.
        let nudge = match verdict.stance.as_str() {
            "endorse" => CONFIDENCE_NUDGE,
            "challenge" | "no_bet" => -CONFIDENCE_NUDGE,
            _ => 0.0,
        };
        let mut nudged = wager.clone();
        nudged.model_prob = (nudged.model_prob + nudge).clamp(0.01, 0.99);
        nudged.edge = nudged.model_prob - nudged.market_implied;
        nudged.thesis = format!(
            "{} | Pundit {}: {}",
            nudged.thesis, verdict.stance, verdict.reason
        );

        if verdict.stance == "no_bet" {
            nudged.status = txodds_types::wager::WagerStatus::NoBet;
            nudged.stake_sol = 0.0;
        }

        let wager_json = serde_json::to_string(&nudged).unwrap_or_default();
        let reason_escaped = verdict.reason.replace('"', "'");

        tracing::info!(
            wager_id = %nudged.wager_id,
            stance = %verdict.stance,
            reason = %verdict.reason,
            nudged_prob = nudged.model_prob,
            "fan-pundit-agent: verdict"
        );

        format!(
            "PUNDIT_REACT_VERDICT stance={} reason=\"{reason_escaped}\" wager={wager_json}",
            verdict.stance,
        )
    }
}

/// Run the Venice LLM tool-calling loop to get a pundit verdict.
/// Falls back to a neutral "endorse" if Venice is unavailable.
async fn run_venice_verdict(wager: &Wager) -> PunditVerdict {
    let client = match rig_venice::client() {
        Ok(c) => c,
        Err(_) => {
            tracing::warn!("fan-pundit-agent: Venice not configured; defaulting to endorse");
            return PunditVerdict {
                stance: "endorse".into(),
                reason: "Venice not configured; defaulting to endorse".into(),
            };
        }
    };

    let model = rig_venice::model_name();
    let agent = client
        .agent(&model)
        .preamble(SYSTEM_PREAMBLE)
        .tool(SubmitPunditVerdict)
        .build();

    let prompt = format!(
        "Selection: {:?}. Model probability: {:.3}. Market-implied probability: {:.3}. Edge: \
         {:.3}. Thesis: {}",
        wager.selection, wager.model_prob, wager.market_implied, wager.edge, wager.thesis,
    );

    let max_rounds: u32 = env_parse("MAX_TOOL_ROUNDS", 3);
    let outcome =
        rig_venice::loop_runner::run_tool_loop(&agent, prompt, SubmitPunditVerdict::NAME, max_rounds, || {})
            .await;

    let Ok(outcome) = outcome else {
        tracing::warn!("fan-pundit-agent: Venice reasoning failed; defaulting to endorse");
        return PunditVerdict {
            stance: "endorse".into(),
            reason: "Venice reasoning failed; defaulting to endorse".into(),
        };
    };

    let Some(final_args) = outcome.final_args else {
        return PunditVerdict {
            stance: "endorse".into(),
            reason: "pundit did not reach a verdict within round cap".into(),
        };
    };

    serde_json::from_value::<PunditVerdict>(final_args).unwrap_or(PunditVerdict {
        stance: "endorse".into(),
        reason: "malformed verdict; defaulting to endorse".into(),
    })
}

/// Extract the `wager=<json>` token via the shared brace-matching extractor
/// — tolerates other keys after the JSON (e.g. the orchestrator's
/// `toolTrail=<json>`, TODO 6e), unlike the old greedy-to-end-of-string
/// parse this replaces.
fn parse_wager(text: &str) -> Option<Wager> {
    serde_json::from_str(wire::json_val(text, "wager")?).ok()
}

fn env_parse<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .json()
        .init();

    let max_wait_ms: u64 = env_parse("FP_MAX_WAIT_MS", 30_000);
    let max_steps: u64 = env_parse("MAX_STEPS", 100_000);

    tracing::info!(agent = "fan-pundit-agent", "starting");

    if let Err(err) = coral_client::run(FanPunditSpecialist, max_wait_ms, max_steps).await {
        tracing::error!(error = %err, "fan-pundit-agent: fatal");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use txodds_types::wager::{Selection, WagerStatus};

    fn sample_wager() -> Wager {
        Wager {
            wager_id: "w-fp-1".into(),
            fixture_id: 42,
            selection: Selection::Home,
            model_prob: 0.55,
            market_implied: 0.50,
            edge: 0.05,
            fair_odds: 1.0 / 0.55,
            stake_sol: 0.01,
            thesis: "home value".into(),
            proof_ref: Some("txoracle:deadbeef".into()),
            status: WagerStatus::Debated,
            debate: None,
            created_at: "2026-07-11T00:00:00Z".into(),
        }
    }

    #[tokio::test]
    async fn handles_pundit_react_requested() {
        let specialist = FanPunditSpecialist;
        let wager_json = serde_json::to_string(&sample_wager()).unwrap();
        let mention = CoralMention {
            thread_id: Some("t-1".into()),
            sender: Some("match-intelligence-agent".into()),
            text: format!("PUNDIT_REACT_REQUESTED wager={wager_json}"),
        };
        // Without VENICE_API_KEY, falls back to endorse.
        std::env::remove_var("VENICE_API_KEY");
        let reply = specialist.handle(mention).await;
        assert!(reply.starts_with("PUNDIT_REACT_VERDICT stance=endorse"));
    }

    #[tokio::test]
    async fn handles_nobet_wager() {
        let specialist = FanPunditSpecialist;
        let mut wager = sample_wager();
        wager.status = WagerStatus::NoBet;
        let wager_json = serde_json::to_string(&wager).unwrap();
        let mention = CoralMention {
            thread_id: Some("t-2".into()),
            sender: Some("match-intelligence-agent".into()),
            text: format!("PUNDIT_REACT_REQUESTED wager={wager_json}"),
        };
        let reply = specialist.handle(mention).await;
        assert!(reply.contains("no live proposal"));
    }

    #[tokio::test]
    async fn handles_delegation_with_tool_trail() {
        // The orchestrator now carries the round's reasoning trail on the
        // delegation (TODO 6e) — `toolTrail=<json>` precedes the trailing
        // `wager=<json>` and must not disturb the wager parse. NoBet wager
        // so the reply path needs no Venice.
        let specialist = FanPunditSpecialist;
        let mut wager = sample_wager();
        wager.status = WagerStatus::NoBet;
        let wager_json = serde_json::to_string(&wager).unwrap();
        let trail = r#"[{"agent":"fan-pundit-agent","tool":"submit_pundit_verdict","result":{"stance":"endorse","reason":"ok"}}]"#;
        let mention = CoralMention {
            thread_id: Some("t-2b".into()),
            sender: Some("match-intelligence-agent".into()),
            text: format!("PUNDIT_REACT_REQUESTED toolTrail={trail} wager={wager_json}"),
        };
        let reply = specialist.handle(mention).await;
        assert!(reply.contains("no live proposal"));
    }

    #[tokio::test]
    async fn ignores_non_delegation_mentions() {
        let specialist = FanPunditSpecialist;
        let mention = CoralMention {
            thread_id: Some("t-3".into()),
            sender: Some("someone".into()),
            text: "HELLO round=1".into(),
        };
        assert_eq!(specialist.handle(mention).await, "");
    }
}
