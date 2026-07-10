//! Venice AI reasoning agent: tool-calling loop that decides sharpness and
//! produces a structured final answer.
//!
//! ## rig-venice ROADMAP.md Phase 2 — structured final output
//!
//! Phase 1 ended with the agent answering in free text, parsed only as an
//! opaque `narrative` string. Phase 2 forces the final answer through a tool
//! call instead: the agent must call `submit_signal_decision` to terminate,
//! and its arguments deserialize directly into `SignalDecision` — no
//! text-scraping. Note this does NOT change what gates whether a signal is
//! logged: `compute_sharp_movement`'s own tool result remains the sole
//! authority for `is_sharp_move` / `confidence` / `direction` (see
//! `ToolLoopOutcome::tool_result` in `rig_venice::loop_runner`). `SignalDecision` only carries
//! the model's rationale — giving the model a structured field for its
//! self-reported sharpness assessment too would weaken the Phase 1 invariant
//! that the boolean comes from deterministic tool code, not model prose.

use agent_core::safety::{wrap_untrusted, BudgetGuard};
use rig::agent::Agent;
use rig::completion::ToolDefinition;
use rig::providers::openai;
use rig::tool::Tool;
use rig_venice::tools::{ComputeSharpMovement, FetchActiveFixtures, FetchOddsSnapshot, MovementResult};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::config::Config;
use crate::txline::FixtureSummary;

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub(crate) struct SignalDecision {
    /// One sentence (max 40 words) explaining what the movement likely means
    /// and which side the sharp money is backing. No gambling advice.
    rationale: String,
}

#[derive(Debug)]
pub(crate) struct SubmitError(String);

impl std::fmt::Display for SubmitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "submit_signal_decision error: {}", self.0)
    }
}
impl std::error::Error for SubmitError {}

/// Forces the agent's final answer into a structured shape instead of free
/// text. Calling this tool is how the agent signals "I'm done reasoning."
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct SubmitSignalDecision;

impl Tool for SubmitSignalDecision {
    const NAME: &'static str = "submit_signal_decision";

    type Error = SubmitError;
    type Args = SignalDecision;
    type Output = serde_json::Value;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        serde_json::to_value(schemars::schema_for!(SignalDecision))
            .map(|parameters| ToolDefinition {
                name: Self::NAME.to_owned(),
                description: "Submit your final assessment and end the reasoning session. \
                    Call this exactly once, after calling compute_sharp_movement, with your \
                    one-sentence rationale. Do not call any other tool afterward."
                    .to_owned(),
                parameters,
            })
            .unwrap_or_else(|e| ToolDefinition {
                name: Self::NAME.to_owned(),
                description: format!("submit_signal_decision (schema unavailable: {e})"),
                parameters: serde_json::json!({}),
            })
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        serde_json::to_value(&args).map_err(|e| SubmitError(e.to_string()))
    }
}

pub(crate) type VeniceAgent = Agent<openai::CompletionModel>;

const SYSTEM_PREAMBLE: &str = "\
You are a sports-betting sharp-money analyst monitoring TxLINE odds. You are \
given a single selection's current and previous decimal odds. Decide whether \
this represents a sharp-money movement worth flagging. \
\n\n\
You MUST call the `compute_sharp_movement` tool with the odds you were given \
to get an objective pct_change / is_sharp_move / direction / confidence \
reading — never estimate these numbers yourself. You may also call \
`fetch_odds_snapshot` or `fetch_active_fixtures` first if you want fresher \
context before assessing. \
\n\n\
Once you have called compute_sharp_movement, you MUST call \
`submit_signal_decision` exactly once with a one-sentence (max 40 words) \
rationale explaining what the movement likely means and which side the \
sharp money is backing. Do not give gambling advice. Do not respond in plain \
text and do not call any tool after submit_signal_decision.";

/// Build the Venice reasoning agent with the read-only TxLINE tools attached.
///
/// Per rig-venice ROADMAP.md, every Venice call in this binary now goes
/// through `rig_venice::client()` — there is no separate raw-reqwest Venice
/// integration left in this file.
pub(crate) fn build_reasoning_agent(config: &Config) -> Result<VeniceAgent, String> {
    let rig_client = rig_venice::client().map_err(|e| e.to_string())?;
    let model = rig_venice::model_name();

    // rig-venice's tools use `{api_base}/fixtures...`; this binary's fixtures
    // live under `{TXLINE_API_BASE}/worldcup/fixtures...`. Bake the prefix in
    // here so the tool-call URLs match what `fetch_live_fixtures` /
    // `fetch_odds` already use, rather than silently hitting a different path.
    let tool_api_base = format!("{}/worldcup", config.api_base);

    Ok(rig_client
        .agent(&model)
        .preamble(SYSTEM_PREAMBLE)
        .tool(FetchOddsSnapshot::new(tool_api_base.clone(), config.api_key.clone()))
        .tool(FetchActiveFixtures::new(tool_api_base, config.api_key.clone()))
        .tool(ComputeSharpMovement { threshold_pct: config.odds_move_threshold_pct })
        .tool(SubmitSignalDecision)
        .build())
}

/// Run the agent's tool-calling loop for one (fixture, market, selection)
/// candidate and pull the `compute_sharp_movement` result out of the trace.
///
/// Returns `None` if the agent never called `compute_sharp_movement` (i.e.
/// judged the pair not worth even checking) or if the Venice call failed —
/// both are non-fatal per signal, matching the original narration step's
/// "failures here are non-fatal" contract.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn assess_movement(
    agent: &VeniceAgent,
    budget: &BudgetGuard,
    max_rounds: u32,
    fixture: &FixtureSummary,
    market_key: &str,
    selection: &str,
    current_odds: f64,
    previous_odds: f64,
) -> Option<(MovementResult, Option<String>)> {
    let safe_fixture_name = wrap_untrusted("fixture_name", &fixture.name);
    let safe_selection = wrap_untrusted("selection_name", selection);
    let safe_market = wrap_untrusted("market_key", market_key);

    let user_prompt = format!(
        "Fixture: {safe_fixture_name}\nMarket: {safe_market}\nSelection: {safe_selection}\n\
         Current odds: {current_odds:.3}\nPrevious odds: {previous_odds:.3}"
    );

    // The tool-calling loop itself now lives in `rig_venice::loop_runner` —
    // extracted there once this binary stopped being the only consumer (see
    // rig-venice ROADMAP.md, "Loose ends" / Phase 4). Only the
    // sharp-movement-detector-specific parsing (pull `compute_sharp_movement`
    // and `submit_signal_decision` out of the generic trace) stays local.
    let outcome = rig_venice::loop_runner::run_tool_loop(
        agent,
        user_prompt,
        SubmitSignalDecision::NAME,
        max_rounds,
        || budget.record_tool_call(),
    )
    .await;

    match outcome {
        Ok(outcome) => {
            let movement: MovementResult =
                serde_json::from_value(outcome.tool_result("compute_sharp_movement")?.clone()).ok()?;
            let decision: Option<SignalDecision> =
                outcome.final_args.and_then(|v| serde_json::from_value(v).ok());
            Some((movement, decision.map(|d| d.rationale)))
        }
        Err(e) => {
            warn!(error = %e, "Venice agent reasoning failed — signal skipped");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_loop_outcome_finds_latest_compute_sharp_movement_call() {
        let outcome = rig_venice::loop_runner::ToolLoopOutcome {
            tool_calls: vec![
                (
                    "fetch_odds_snapshot".to_owned(),
                    serde_json::json!({ "fixture_id": 1 }),
                ),
                (
                    "compute_sharp_movement".to_owned(),
                    serde_json::json!({
                        "pct_change": 5.0,
                        "is_sharp_move": true,
                        "direction": "shortening",
                        "confidence": 0.6
                    }),
                ),
            ],
            final_args: Some(serde_json::json!({ "rationale": "test" })),
        };
        let Some(value) = outcome.tool_result("compute_sharp_movement") else {
            panic!("compute_sharp_movement result should be present in trace");
        };
        let Ok(movement) = serde_json::from_value::<MovementResult>(value.clone()) else {
            panic!("expected a valid MovementResult json value");
        };
        assert!(movement.is_sharp_move);
        assert_eq!(movement.direction, "shortening");
    }

    #[test]
    fn tool_loop_outcome_none_when_tool_never_called() {
        let outcome = rig_venice::loop_runner::ToolLoopOutcome {
            tool_calls: vec![],
            final_args: Some(serde_json::json!({ "rationale": "skip" })),
        };
        assert!(outcome.tool_result("compute_sharp_movement").is_none());
    }

    // ── Phase 3 eval harness: offline replay against a mock Venice endpoint ──
    //
    // rig-venice ROADMAP.md Phase 3 asks for an eval harness that replays
    // historical odds movements through the agent loop. There's no live
    // TxLINE/Venice access available to build that against real historical
    // data, so this is the offline-inspectable substitute: a minimal mock
    // OpenAI-compatible HTTP server (no live credentials needed) scripted to
    // return a `compute_sharp_movement` tool call followed by a
    // `submit_signal_decision` tool call, driving the *real*
    // `build_reasoning_agent` / `assess_movement` / `rig_venice::loop_runner::run_tool_loop` code
    // path end to end. This is what should be extended with more scripted
    // response sequences (and eventually real historical data) before this
    // agent is trusted on a live poll loop.

    /// Spin up a minimal mock server on an OS-assigned port that answers
    /// every request with the next body in `bodies` (repeating the last body
    /// once exhausted). Good enough for scripting a fixed sequence of
    /// OpenAI-compatible chat-completion responses; not a general HTTP mock.
    async fn spawn_mock_venice(bodies: Vec<String>) -> String {
        let Ok(listener) = tokio::net::TcpListener::bind("127.0.0.1:0").await else {
            panic!("failed to bind mock Venice listener");
        };
        let Ok(addr) = listener.local_addr() else {
            panic!("failed to read mock Venice listener address");
        };
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let bodies = std::sync::Arc::new(bodies);

        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let counter = counter.clone();
                let bodies = bodies.clone();
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut buf = vec![0u8; 16_384];
                    let _ = socket.read(&mut buf).await;

                    let idx = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    let last = bodies.last().cloned().unwrap_or_default();
                    let body = bodies.get(idx).cloned().unwrap_or(last);

                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.shutdown().await;
                });
            }
        });

        format!("http://{addr}")
    }

    #[tokio::test]
    async fn phase3_agent_loop_replay_against_mock_venice() {
        // Scripted two-round transcript: round 1 the agent calls
        // compute_sharp_movement with the odds it was given; round 2 it
        // submits its structured decision. Matches the OpenAI chat-completion
        // response shape rig-core's openai provider deserializes.
        let round1 = r#"{"id":"resp1","object":"chat.completion","created":0,"model":"kimi-k2-7-code","choices":[{"index":0,"message":{"role":"assistant","content":[],"refusal":null,"audio":null,"name":null,"tool_calls":[{"id":"call_1","type":"function","function":{"name":"compute_sharp_movement","arguments":"{\"selection\":\"Home\",\"current_odds\":1.90,\"previous_odds\":2.00,\"market_key\":\"1x2\"}"}}]},"logprobs":null,"finish_reason":"tool_calls"}],"usage":null}"#;
        let round2 = r#"{"id":"resp2","object":"chat.completion","created":0,"model":"kimi-k2-7-code","choices":[{"index":0,"message":{"role":"assistant","content":[],"refusal":null,"audio":null,"name":null,"tool_calls":[{"id":"call_2","type":"function","function":{"name":"submit_signal_decision","arguments":"{\"rationale\":\"Sharp money is backing Home after a shortening move.\"}"}}]},"logprobs":null,"finish_reason":"tool_calls"}],"usage":null}"#;

        let base_url = spawn_mock_venice(vec![round1.to_owned(), round2.to_owned()]).await;

        // Safe: this is the only test in this binary that touches Venice env
        // vars, so there's no cross-test race on the process environment.
        std::env::set_var("VENICE_API_KEY", "test-key");
        std::env::set_var("VENICE_BASE_URL", base_url);
        std::env::set_var("VENICE_MODEL", "kimi-k2-7-code");

        let config = Config {
            api_base: "https://txline.example.invalid/api/v1".to_owned(),
            api_key: "txline-test-key".to_owned(),
            poll_interval_secs: 60,
            odds_move_threshold_pct: 4.0,
            confidence_gate: 0.55,
            max_steps: 500,
            max_tool_rounds: 6,
            signal_log_path: "unused-in-this-test.jsonl".to_owned(),
        };

        let Ok(agent) = build_reasoning_agent(&config) else {
            panic!("failed to build reasoning agent against mock Venice server");
        };
        let budget = BudgetGuard::default_devnet();
        let fixture = FixtureSummary {
            id: 1,
            name: "Test FC vs Mock United".to_owned(),
            status: "live".to_owned(),
        };

        let result = assess_movement(
            &agent,
            &budget,
            config.max_tool_rounds,
            &fixture,
            "1x2",
            "Home",
            1.90,
            2.00,
        )
        .await;

        let Some((movement, rationale)) = result else {
            panic!("expected an assessment from the mock Venice loop");
        };
        assert!(movement.is_sharp_move);
        assert_eq!(movement.direction, "shortening");
        let Some(rationale) = rationale else {
            panic!("expected a rationale from submit_signal_decision");
        };
        assert!(rationale.contains("Home"));
    }
}
