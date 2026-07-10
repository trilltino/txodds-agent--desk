//! Shared multi-turn tool-calling loop.
//!
//! `rig-core` 0.9's built-in `Agent::chat`/`prompt` only ever executes a
//! single tool call before returning it as the final answer — it does not
//! loop the result back to the model for further reasoning (see
//! `crates/rig-venice/ROADMAP.md` Phase 1 for how this was discovered).
//! Every agent in this workspace that needs a real multi-turn loop hand-rolls
//! one via the `Completion` trait; this module is that hand-rolled loop,
//! extracted here once a second consumer (`native/src/services/agent`)
//! needed the exact same pattern `sharp-movement-detector` already had.
//!
//! The convention: the agent must terminate by calling a designated "final"
//! tool (its arguments become the structured result) rather than by
//! responding in plain text. This mirrors rig-venice ROADMAP.md Phase 2 —
//! forced structured output instead of text-scraping.

use rig::agent::Agent;
use rig::completion::Completion;
use rig::message::{AssistantContent, Message, ToolResultContent, UserContent};
use rig::providers::openai;
use rig::OneOrMany;

/// Every tool call the agent made, plus the arguments of the final tool call
/// (the one matching `final_tool_name`), if it made one before running out
/// of rounds.
pub struct ToolLoopOutcome {
    pub tool_calls: Vec<(String, serde_json::Value)>,
    pub final_args: Option<serde_json::Value>,
}

impl ToolLoopOutcome {
    /// The most recent result of the named tool, if it was called at all.
    /// Useful for pulling a deterministic tool's own output out of the trace
    /// rather than trusting the model's self-reported summary of it.
    #[must_use]
    pub fn tool_result(&self, name: &str) -> Option<&serde_json::Value> {
        self.tool_calls.iter().rev().find(|(n, _)| n == name).map(|(_, v)| v)
    }
}

#[derive(Debug)]
pub struct ToolLoopError(pub String);

impl std::fmt::Display for ToolLoopError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "tool loop error: {}", self.0)
    }
}
impl std::error::Error for ToolLoopError {}

/// Drive `agent`'s tool-calling loop to completion.
///
/// `on_step` is called once per Venice completion request and once per tool
/// execution — callers that track a budget/step cap (e.g.
/// `agent_core::safety::BudgetGuard::record_tool_call`) pass that in; callers
/// that don't care pass a no-op closure.
///
/// Terminates when the model calls `final_tool_name` (returning its
/// arguments as `final_args`), when it responds with no tool calls at all
/// (treated as "gave up without an answer" — `final_args: None`), or when
/// `max_rounds` is exhausted.
pub async fn run_tool_loop(
    agent: &Agent<openai::CompletionModel>,
    initial_prompt: String,
    final_tool_name: &str,
    max_rounds: u32,
    mut on_step: impl FnMut(),
) -> Result<ToolLoopOutcome, ToolLoopError> {
    let mut history: Vec<Message> = Vec::new();
    let mut next_prompt = Message::user(initial_prompt);
    let mut tool_calls = Vec::new();

    for _round in 0..max_rounds {
        on_step();

        let response = agent
            .completion(next_prompt.clone(), history.clone())
            .await
            .map_err(|e| ToolLoopError(format!("venice completion request failed: {e}")))?
            .send()
            .await
            .map_err(|e| ToolLoopError(format!("venice completion send failed: {e}")))?;

        let contents: Vec<AssistantContent> = response.choice.into_iter().collect();
        let requested: Vec<_> = contents
            .iter()
            .filter_map(|c| match c {
                AssistantContent::ToolCall(tc) => Some(tc.clone()),
                AssistantContent::Text(_) => None,
            })
            .collect();

        if requested.is_empty() {
            return Ok(ToolLoopOutcome { tool_calls, final_args: None });
        }

        history.push(next_prompt.clone());
        let assistant_content =
            OneOrMany::many(contents).map_err(|e| ToolLoopError(format!("empty response: {e}")))?;
        history.push(Message::Assistant { content: assistant_content });

        let mut results = Vec::with_capacity(requested.len());
        let mut final_args: Option<serde_json::Value> = None;
        for call in &requested {
            on_step();
            let args = call.function.arguments.to_string();

            if call.function.name == final_tool_name && final_args.is_none() {
                final_args = Some(call.function.arguments.clone());
            }

            let output = agent.tools.call(&call.function.name, args).await;
            let output_str = match output {
                Ok(s) => {
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&s) {
                        tool_calls.push((call.function.name.clone(), value));
                    }
                    s
                }
                Err(e) => format!("tool error: {e}"),
            };
            results.push(UserContent::tool_result(
                call.id.clone(),
                OneOrMany::one(ToolResultContent::text(output_str)),
            ));
        }

        if let Some(final_args) = final_args {
            return Ok(ToolLoopOutcome { tool_calls, final_args: Some(final_args) });
        }

        next_prompt = Message::User {
            content: OneOrMany::many(results).map_err(|e| ToolLoopError(format!("empty results: {e}")))?,
        };
    }

    Ok(ToolLoopOutcome { tool_calls, final_args: None })
}
