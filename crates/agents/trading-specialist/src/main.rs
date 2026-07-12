//! trading-specialist — real, independent CoralOS participant for the
//! Trading-track handoff. Registered as `sharp-movement-detector` (see
//! `coral-agent.toml`) — that CoralOS identity is fixed by `protocol.rs`
//! and the existing message protocol, but this crate is named differently
//! to avoid colliding with the already-real `crates/agents/sharp-movement-detector`
//! (TxLINE odds-movement polling, a different job: that one watches the
//! market itself, this one reacts to a signal `match-intelligence-agent`
//! already derived and delegated).
//!
//! Wire grammar (flat `VERB key=value` — same convention as proof-guard-agent):
//!
//!   TRADE_REQUESTED  signal=<json-AgentSignal> decision=<json-AgentDecision>
//!   TRADE_VERDICT    status=<simulated|declined> reason="..." positionId=<string|none> sizeSol=<f64>
//!
//! Deterministic, not LLM-backed — this specialist's job (per the original
//! handoff acknowledgement text it replaces) is bookkeeping a simulated
//! position, not narrative reasoning. Confidence/severity gate whether a
//! position is simulated at all; sizing is a simple deterministic function
//! of both, capped by `TS_MAX_POSITION_SOL`.

use agent_core::domain::{AgentDecision, AgentSignal, SignalSeverity};
use coral_client::{wire, CoralMention, Specialist};

/// Minimum confidence before a position is worth simulating at all.
const MIN_CONFIDENCE: f64 = 0.4;

struct TradingSpecialist {
    max_position_sol: f64,
}

#[async_trait::async_trait]
impl Specialist for TradingSpecialist {
    fn name(&self) -> &str {
        "sharp-movement-detector"
    }

    async fn handle(&self, mention: CoralMention) -> String {
        if wire::verb(&mention.text) != "TRADE_REQUESTED" {
            tracing::debug!(text = %mention.text, "trading-specialist: ignoring non-delegation mention");
            return String::new();
        }

        let Some(signal) = parse_json::<AgentSignal>(&mention.text, "signal") else {
            tracing::warn!(text = %mention.text, "trading-specialist: missing/malformed signal= payload");
            return "TRADE_VERDICT status=declined reason=\"malformed delegation: no signal payload\" positionId=none sizeSol=0.0".to_string();
        };
        let decision = parse_json::<AgentDecision>(&mention.text, "decision");

        if signal.confidence < MIN_CONFIDENCE {
            tracing::info!(signal_id = %signal.id, confidence = signal.confidence, "trading-specialist: below confidence floor, declining");
            return format!(
                "TRADE_VERDICT status=declined reason=\"confidence {:.2} below floor {:.2}\" positionId=none sizeSol=0.0",
                signal.confidence, MIN_CONFIDENCE
            );
        }

        let size_sol = self.simulated_size(&signal);
        let position_id = uuid::Uuid::new_v4().to_string();

        tracing::info!(
            signal_id = %signal.id,
            fixture_id = signal.fixture_id,
            position_id = %position_id,
            size_sol,
            decision_action = ?decision.as_ref().map(|d| &d.action),
            "trading-specialist: position simulated"
        );

        format!(
            "TRADE_VERDICT status=simulated reason=\"position simulation queued\" positionId={position_id} sizeSol={size_sol:.4}"
        )
    }
}

impl TradingSpecialist {
    /// Deterministic simulated position size: confidence and severity both
    /// scale it, capped by `max_position_sol`. Not a real stake — this
    /// specialist never touches money (see `agent_core::capability`; no
    /// money-moving token is constructed here at all).
    fn simulated_size(&self, signal: &AgentSignal) -> f64 {
        let severity_weight = match signal.severity {
            SignalSeverity::Critical => 1.0,
            SignalSeverity::High => 0.75,
            SignalSeverity::Medium => 0.5,
            SignalSeverity::Low => 0.25,
        };
        (self.max_position_sol * signal.confidence * severity_weight).max(0.0)
    }
}

/// Extract a `key=<json>` token via the shared brace-matching extractor
/// (`coral_client::wire::json_val`) — string-aware, unlike the local
/// brace-counting copy this replaces, which broke on a `{` inside a
/// rationale string.
fn parse_json<T: serde::de::DeserializeOwned>(text: &str, key: &str) -> Option<T> {
    serde_json::from_str(wire::json_val(text, key)?).ok()
}

fn env_parse<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .json()
        .init();

    let max_wait_ms: u64 = env_parse("TS_MAX_WAIT_MS", 30_000);
    let max_steps: u64 = env_parse("MAX_STEPS", 100_000);
    let max_position_sol: f64 = env_parse("TS_MAX_POSITION_SOL", 0.05);

    tracing::info!(agent = "sharp-movement-detector", "trading-specialist: starting");

    let specialist = TradingSpecialist { max_position_sol };

    if let Err(err) = coral_client::run(specialist, max_wait_ms, max_steps).await {
        tracing::error!(error = %err, "trading-specialist: fatal");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::domain::{AgentAction, ExecutionStatus, PolicyCheck, SignalType};
    use std::collections::BTreeMap;

    fn sample_signal(confidence: f64) -> AgentSignal {
        AgentSignal {
            id: "sig-1".into(),
            fixture_id: 42,
            source_event_id: "evt-1".into(),
            signal_type: SignalType::SharpOddsMove,
            severity: SignalSeverity::High,
            confidence,
            features: BTreeMap::new(),
            rationale: "test".into(),
            created_at: "2026-07-11T00:00:00Z".into(),
        }
    }

    fn sample_decision() -> AgentDecision {
        AgentDecision {
            id: "dec-1".into(),
            signal_id: "sig-1".into(),
            action: AgentAction::SimulatePosition,
            confidence: 0.8,
            policy_checks: vec![PolicyCheck {
                name: "check".into(),
                passed: true,
                detail: "ok".into(),
            }],
            explanation: "test".into(),
            execution_status: ExecutionStatus::Pending,
            created_at: "2026-07-11T00:00:00Z".into(),
        }
    }

    #[tokio::test]
    async fn simulates_position_above_confidence_floor() {
        let specialist = TradingSpecialist { max_position_sol: 0.05 };
        let signal_json = serde_json::to_string(&sample_signal(0.8)).unwrap();
        let decision_json = serde_json::to_string(&sample_decision()).unwrap();
        let mention = CoralMention {
            thread_id: Some("t-1".into()),
            sender: Some("match-intelligence-agent".into()),
            text: format!("TRADE_REQUESTED signal={signal_json} decision={decision_json}"),
        };
        let reply = specialist.handle(mention).await;
        assert!(reply.starts_with("TRADE_VERDICT status=simulated"));
        assert!(reply.contains("positionId="));
    }

    #[tokio::test]
    async fn declines_below_confidence_floor() {
        let specialist = TradingSpecialist { max_position_sol: 0.05 };
        let signal_json = serde_json::to_string(&sample_signal(0.1)).unwrap();
        let mention = CoralMention {
            thread_id: Some("t-2".into()),
            sender: Some("match-intelligence-agent".into()),
            text: format!("TRADE_REQUESTED signal={signal_json}"),
        };
        let reply = specialist.handle(mention).await;
        assert!(reply.starts_with("TRADE_VERDICT status=declined"));
        assert!(reply.contains("positionId=none"));
    }

    #[tokio::test]
    async fn ignores_non_delegation_mentions() {
        let specialist = TradingSpecialist { max_position_sol: 0.05 };
        let mention = CoralMention {
            thread_id: Some("t-3".into()),
            sender: Some("someone".into()),
            text: "HELLO round=1".into(),
        };
        assert_eq!(specialist.handle(mention).await, "");
    }
}
