//! proof-guard-agent — the real, independent wager-consistency gate.
//!
//! A standalone OS process registered with coral-server (see
//! `crates/agents/proof-guard-agent/coral-agent.toml`), launched by
//! coral-server itself, that blocks on its own `wait_for_mention` loop
//! (`coral_client::run`) and replies with the deterministic verdict from
//! `agent_core::proof_guard::verify` — never an LLM, never a debate
//! participant, per `crates/rig-venice/ROADMAP.md`'s own invariant.
//!
//! The orchestrator (`native/`) cannot see this process's reasoning, only
//! the `WAGER_PROOF_VERDICT` message it publishes back on the Coral thread —
//! the property the puppet API structurally could not provide.
//!
//! Wire grammar (flat `VERB key=value` — the real `send_message` MCP tool
//! has no structured-payload field, so the `Wager` rides as a trailing JSON
//! token, the same trick `pay`'s `VERIFY ... payload=<raw>` uses):
//!
//!   WAGER_PROOF_REQUESTED round=<n> wagerId=<id> wager=<json>
//!   WAGER_PROOF_VERDICT   round=<n> wagerId=<id> passed=<bool> wager=<json>

use agent_core::proof_guard::{self, ProofGuardConfig};
use coral_client::{wire, CoralMention, Specialist};
use txodds_types::wager::Wager;

struct ProofGuardSpecialist {
    cfg: ProofGuardConfig,
}

#[async_trait::async_trait]
impl Specialist for ProofGuardSpecialist {
    fn name(&self) -> &str {
        "proof-guard-agent"
    }

    async fn handle(&self, mention: CoralMention) -> String {
        if wire::verb(&mention.text) != "WAGER_PROOF_REQUESTED" {
            tracing::debug!(text = %mention.text, "proof-guard-agent: ignoring non-delegation mention");
            return String::new();
        }

        let round = wire::num(&mention.text, "round").unwrap_or(0.0) as u64;

        let Some(wager) = parse_wager(&mention.text) else {
            tracing::warn!(text = %mention.text, "proof-guard-agent: WAGER_PROOF_REQUESTED missing/malformed wager= payload");
            return format!(
                "WAGER_PROOF_VERDICT round={round} wagerId=unknown passed=false reason=\"malformed delegation: no wager payload\""
            );
        };

        let proof_ref = wager.proof_ref.clone();
        let verdict = proof_guard::verify(&wager, proof_ref.as_deref(), &self.cfg);

        let wager_json = serde_json::to_string(&verdict.wager).unwrap_or_default();
        let reason = if verdict.passed {
            String::new()
        } else {
            format!(" reason=\"{}\"", verdict.failures.join("; ").replace('"', "'"))
        };

        tracing::info!(
            wager_id = %verdict.wager.wager_id,
            passed = verdict.passed,
            failures = verdict.failures.len(),
            "proof-guard-agent: verdict"
        );

        format!(
            "WAGER_PROOF_VERDICT round={round} wagerId={} passed={}{reason} wager={wager_json}",
            verdict.wager.wager_id, verdict.passed,
        )
    }
}

/// Extract the `wager=<json>` token via the shared brace-matching extractor
/// — tolerates other keys after the JSON (e.g. the orchestrator's
/// `toolTrail=<json>`, TODO 6e), unlike the old greedy-to-end-of-string
/// parse this replaces.
fn parse_wager(text: &str) -> Option<Wager> {
    serde_json::from_str(wire::json_val(text, "wager")?).ok()
}

struct Config {
    consistency_tol: f64,
    max_devnet_spend_sol: f64,
    max_wait_ms: u64,
    max_steps: u64,
}

impl Config {
    fn from_env() -> Self {
        Self {
            consistency_tol: env_parse("PG_CONSISTENCY_TOL", proof_guard::DEFAULT_CONSISTENCY_TOL),
            max_devnet_spend_sol: env_parse(
                "PG_MAX_DEVNET_SPEND_SOL",
                proof_guard::DEFAULT_MAX_DEVNET_SPEND_SOL,
            ),
            max_wait_ms: env_parse("PG_MAX_WAIT_MS", 30_000.0) as u64,
            max_steps: env_parse("MAX_STEPS", 100_000.0) as u64,
        }
    }
}

fn env_parse(key: &str, default: f64) -> f64 {
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

    let config = Config::from_env();
    let specialist = ProofGuardSpecialist {
        cfg: ProofGuardConfig {
            consistency_tol: config.consistency_tol,
            max_devnet_spend_sol: config.max_devnet_spend_sol,
        },
    };

    if let Err(err) = coral_client::run(specialist, config.max_wait_ms, config.max_steps).await {
        tracing::error!(error = %err, "proof-guard-agent: fatal");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use txodds_types::wager::{Selection, WagerStatus};

    fn sample_wager() -> Wager {
        Wager {
            wager_id: "w-1".into(),
            fixture_id: 42,
            selection: Selection::Home,
            model_prob: 0.55,
            market_implied: 0.50,
            edge: 0.05,
            fair_odds: 1.0 / 0.55,
            stake_sol: 0.01,
            thesis: "test".into(),
            proof_ref: Some("txoracle:deadbeef1234".into()),
            status: WagerStatus::Debated,
            debate: None,
            created_at: "2026-07-11T00:00:00Z".into(),
        }
    }

    #[tokio::test]
    async fn handles_wager_proof_requested() {
        let specialist = ProofGuardSpecialist {
            cfg: ProofGuardConfig {
                consistency_tol: proof_guard::DEFAULT_CONSISTENCY_TOL,
                max_devnet_spend_sol: proof_guard::DEFAULT_MAX_DEVNET_SPEND_SOL,
            },
        };
        let wager_json = serde_json::to_string(&sample_wager()).unwrap();
        let mention = CoralMention {
            thread_id: Some("t-1".into()),
            sender: Some("match-intelligence-agent".into()),
            text: format!("WAGER_PROOF_REQUESTED round=1 wagerId=w-1 wager={wager_json}"),
        };
        let reply = specialist.handle(mention).await;
        assert!(reply.starts_with("WAGER_PROOF_VERDICT round=1 wagerId=w-1 passed=true"));
    }

    #[tokio::test]
    async fn handles_delegation_with_tool_trail() {
        // The orchestrator now carries the round's reasoning trail on the
        // delegation (TODO 6e) — `toolTrail=<json>` precedes the trailing
        // `wager=<json>` and must not disturb the wager parse.
        let specialist = ProofGuardSpecialist {
            cfg: ProofGuardConfig {
                consistency_tol: proof_guard::DEFAULT_CONSISTENCY_TOL,
                max_devnet_spend_sol: proof_guard::DEFAULT_MAX_DEVNET_SPEND_SOL,
            },
        };
        let wager_json = serde_json::to_string(&sample_wager()).unwrap();
        let trail = r#"[{"agent":"match-intelligence-agent","tool":"compute_fair_probability","result":{"home":0.5}}]"#;
        let mention = CoralMention {
            thread_id: Some("t-1b".into()),
            sender: Some("match-intelligence-agent".into()),
            text: format!(
                "WAGER_PROOF_REQUESTED round=1 wagerId=w-1 toolTrail={trail} wager={wager_json}"
            ),
        };
        let reply = specialist.handle(mention).await;
        assert!(reply.starts_with("WAGER_PROOF_VERDICT round=1 wagerId=w-1 passed=true"));
    }

    #[tokio::test]
    async fn ignores_non_delegation_mentions() {
        let specialist = ProofGuardSpecialist {
            cfg: ProofGuardConfig {
                consistency_tol: proof_guard::DEFAULT_CONSISTENCY_TOL,
                max_devnet_spend_sol: proof_guard::DEFAULT_MAX_DEVNET_SPEND_SOL,
            },
        };
        let mention = CoralMention {
            thread_id: Some("t-1".into()),
            sender: Some("someone-else".into()),
            text: "HELLO round=1".into(),
        };
        assert_eq!(specialist.handle(mention).await, "");
    }
}
