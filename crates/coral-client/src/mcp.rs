//! `CoralMcpAgent` — a real MCP participant in a CoralOS session.
//!
//! Direct port of `pay`'s `packages/agent-runtime/src/coral/mcp.ts`
//! (`CoralMcpAgent`): connect -> list_tools -> loop(wait_for_mention ->
//! handler -> send_message). Tool names are discovered by substring match
//! against whatever coral-server actually exposes, exactly like the
//! TypeScript client does — do not hardcode exact tool names, coral-server's
//! naming has already been observed to vary (`wait_for_mention`, not the
//! `wait_for_mentions` plural earlier ports of this loop assumed).

use rmcp::model::CallToolRequestParam;
use rmcp::service::{RoleClient, RunningService};
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::ServiceExt;
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::CoralClientError;

/// A single inbound message addressed (at least in part) to this agent.
#[derive(Debug, Clone, Default)]
pub struct CoralMention {
    pub thread_id: Option<String>,
    pub sender: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct CoralMcpConfig {
    pub connection_url: String,
    pub agent_name: String,
    pub version: String,
}

struct ToolNames {
    wait_for_mention: String,
    wait_for_agent: String,
    send_message: String,
    create_thread: String,
}

pub struct CoralMcpAgent {
    config: CoralMcpConfig,
    service: RunningService<RoleClient, ()>,
    tools: ToolNames,
}

impl CoralMcpAgent {
    /// Connect to CoralOS over streamable-HTTP MCP and discover tool names.
    /// Mirrors `CoralMcpAgent.connect()` in the TypeScript client.
    pub async fn connect(config: CoralMcpConfig) -> Result<Self, CoralClientError> {
        let transport = StreamableHttpClientTransport::from_uri(config.connection_url.clone());
        let service = ()
            .serve(transport)
            .await
            .map_err(|err| CoralClientError::Connect(err.to_string()))?;

        let tools_result = service
            .list_tools(Default::default())
            .await
            .map_err(|err| CoralClientError::Mcp(err.to_string()))?;
        let names: Vec<String> = tools_result
            .tools
            .iter()
            .map(|t| t.name.to_string())
            .collect();
        tracing::info!(agent = %config.agent_name, tools = ?names, "coral-client: connected");

        let tools = ToolNames {
            wait_for_mention: names
                .iter()
                .find(|n| n.contains("wait_for_mention"))
                .cloned()
                .unwrap_or_else(|| "coral_wait_for_mention".to_string()),
            wait_for_agent: names
                .iter()
                .find(|n| n.contains("wait_for_agent"))
                .cloned()
                .unwrap_or_else(|| "coral_wait_for_agent".to_string()),
            send_message: names
                .iter()
                .find(|n| n.ends_with("send_message"))
                .cloned()
                .unwrap_or_else(|| "coral_send_message".to_string()),
            create_thread: names
                .iter()
                .find(|n| n.contains("create_thread"))
                .cloned()
                .unwrap_or_else(|| "coral_create_thread".to_string()),
        };
        tracing::info!(
            wait = %tools.wait_for_mention,
            send = %tools.send_message,
            "coral-client: using tools"
        );

        Ok(Self {
            config,
            service,
            tools,
        })
    }

    /// Block until a mention arrives, or `None` on timeout. `max_wait_ms`
    /// defaults to 30s server-side, matching the TypeScript/Python clients.
    pub async fn wait_for_mention(
        &self,
        max_wait_ms: u64,
    ) -> Result<Option<CoralMention>, CoralClientError> {
        let result = self
            .service
            .call_tool(CallToolRequestParam {
                name: self.tools.wait_for_mention.clone().into(),
                arguments: Some(object(serde_json::json!({
                    "maxWaitMs": max_wait_ms,
                    "currentUnixTime": now_unix_ms(),
                }))),
            })
            .await
            .map_err(|err| CoralClientError::Mcp(err.to_string()))?;

        Ok(parse_mention(&extract_text(&result.content)))
    }

    /// Like [`wait_for_mention`], but skips mentions in other threads.
    /// Polls in bounded slices so it can honour `max_wait_ms` overall
    /// while re-checking `thread_id` on each arrival.
    pub async fn wait_for_mention_in_thread(
        &self,
        thread_id: &str,
        max_wait_ms: u64,
    ) -> Result<Option<CoralMention>, CoralClientError> {
        let deadline = SystemTime::now() + std::time::Duration::from_millis(max_wait_ms);
        loop {
            let remaining = deadline
                .duration_since(SystemTime::now())
                .unwrap_or_default();
            if remaining.is_zero() {
                return Ok(None);
            }
            let slice_ms = remaining.as_millis().clamp(1_000, 15_000) as u64;
            if let Some(mention) = self.wait_for_mention(slice_ms).await? {
                if mention.thread_id.as_deref() == Some(thread_id) {
                    return Ok(Some(mention));
                }
            }
        }
    }

    /// Block until a message from `agent_name` arrives (`coral_wait_for_agent`).
    pub async fn wait_for_agent(
        &self,
        agent_name: &str,
        max_wait_ms: u64,
    ) -> Result<Option<CoralMention>, CoralClientError> {
        let result = self
            .service
            .call_tool(CallToolRequestParam {
                name: self.tools.wait_for_agent.clone().into(),
                arguments: Some(object(serde_json::json!({
                    "agentName": agent_name,
                    "maxWaitMs": max_wait_ms,
                    "currentUnixTime": now_unix_ms(),
                }))),
            })
            .await
            .map_err(|err| CoralClientError::Mcp(err.to_string()))?;

        Ok(parse_mention(&extract_text(&result.content)))
    }

    /// Send a message into a thread. Only `content` (string) and `mentions`
    /// (agent names to @mention) exist on the real transport — there is no
    /// separate structured-payload channel, so any structured data must be
    /// encoded into `content` itself (see `txodds-agent-desk`'s flat
    /// `VERB key=value` wire grammar, mirroring `pay`'s market protocol).
    pub async fn send_message(
        &self,
        content: &str,
        thread_id: &str,
        mentions: &[&str],
    ) -> Result<(), CoralClientError> {
        self.service
            .call_tool(CallToolRequestParam {
                name: self.tools.send_message.clone().into(),
                arguments: Some(object(serde_json::json!({
                    "threadId": thread_id,
                    "content": content,
                    "mentions": mentions,
                }))),
            })
            .await
            .map_err(|err| CoralClientError::Mcp(err.to_string()))?;
        Ok(())
    }

    /// Create a new thread and return its id.
    pub async fn create_thread(
        &self,
        thread_name: &str,
        participant_names: &[&str],
    ) -> Result<String, CoralClientError> {
        let result = self
            .service
            .call_tool(CallToolRequestParam {
                name: self.tools.create_thread.clone().into(),
                arguments: Some(object(serde_json::json!({
                    "threadName": thread_name,
                    "participantNames": participant_names,
                }))),
            })
            .await
            .map_err(|err| CoralClientError::Mcp(err.to_string()))?;

        let text = extract_text(&result.content);
        let thread_id = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| {
                v.pointer("/thread/id")
                    .or_else(|| v.get("threadId"))
                    .or_else(|| v.get("id"))
                    .and_then(Value::as_str)
                    .map(ToString::to_string)
            })
            .unwrap_or(text);
        Ok(thread_id)
    }

    pub fn agent_name(&self) -> &str {
        &self.config.agent_name
    }

    pub async fn disconnect(self) -> Result<(), CoralClientError> {
        self.service
            .cancel()
            .await
            .map_err(|err| CoralClientError::Mcp(err.to_string()))?;
        Ok(())
    }
}

fn object(value: Value) -> rmcp::model::JsonObject {
    match value {
        Value::Object(map) => map,
        _ => serde_json::Map::new(),
    }
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn extract_text(content: &[rmcp::model::Content]) -> String {
    content
        .iter()
        .filter_map(|c| c.as_text())
        .map(|t| t.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

/// Parse the JSON blob `coral_wait_for_mention` / `coral_wait_for_agent`
/// return into a [`CoralMention`]. Port of `parseMention()` in the
/// TypeScript client — same tolerant shape-probing (nested `messages[0]`,
/// `message`, or flat top-level fields), since coral-server's exact
/// response shape isn't part of any stable contract this crate controls.
fn parse_mention(raw: &str) -> Option<CoralMention> {
    if raw.is_empty() || raw == "null" || raw == "{}" || raw == "[]" {
        return None;
    }

    let mut thread_id: Option<String> = None;
    let mut sender: Option<String> = None;
    let mut message_text = raw.to_string();

    if let Ok(data) = serde_json::from_str::<Value>(raw) {
        let status = data.get("status").and_then(Value::as_str);
        if matches!(status, Some("Timeout reached") | Some("timeout")) {
            return None;
        }

        thread_id = str_field(&data, &["threadId", "thread_id"]).or(thread_id);
        sender = str_field(&data, &["senderName", "sender", "senderId", "from"]).or(sender);

        if let Some(messages) = data.get("messages").and_then(Value::as_array) {
            if let Some(m0) = messages.first() {
                thread_id = str_field(m0, &["threadId", "thread_id"]).or(thread_id);
                sender = str_field(m0, &["senderName", "sender", "senderId"]).or(sender);
                message_text = str_field(m0, &["text", "content"]).unwrap_or_else(|| raw.to_string());
            }
        }

        if let Some(m) = data.get("message").filter(|v| v.is_object()) {
            thread_id = str_field(m, &["threadId", "thread_id"]).or(thread_id);
            sender = str_field(m, &["senderName", "sender", "senderId"]).or(sender);
            message_text = str_field(m, &["text", "content"]).unwrap_or_else(|| raw.to_string());
        }

        if message_text == raw {
            message_text = str_field(&data, &["text", "content"]).unwrap_or_else(|| raw.to_string());
        }
    }

    if message_text.is_empty() {
        return None;
    }
    Some(CoralMention {
        thread_id,
        sender,
        text: message_text,
    })
}

fn str_field(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|k| value.get(k).and_then(Value::as_str))
        .map(ToString::to_string)
}
