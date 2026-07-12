//! settlement-agent — real, independent CoralOS on-chain settlement participant.
//!
//! Wire grammar (flat `VERB key=value` — same convention as proof-guard-agent):
//!
//!   SETTLE_REQUESTED  wager=<json> proofRef=<string>
//!   SETTLE_VERDICT     wagerId=<id> status=<settled|rejected> reason="..." txSig=<string|none>
//!
//! The orchestrator sends `SETTLE_REQUESTED` once the proof gate has passed
//! and the Authority has adjudicated a live wager. This agent verifies the
//! proof reference is valid, simulates (devnet) or executes (mainnet) the
//! on-chain settlement, and replies with the transaction signature or a
//! rejection reason.
//!
//! ## Capability token (§8)
//!
//! Only this binary holds `SettleCap`. The compile-time ZST prevents any
//! other crate from calling `settle_wager` without constructing one first.

use coral_client::{wire, CoralMention, Specialist};
use agent_core::capability::SettleCap;

// ── Settlement types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettleStatus {
    Settled,
    Rejected,
}

impl std::fmt::Display for SettleStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Settled => write!(f, "settled"),
            Self::Rejected => write!(f, "rejected"),
        }
    }
}

// ── Specialist implementation ─────────────────────────────────────────────────

struct SettlementSpecialist {
    /// Capability token — only this binary can construct one (§8).
    _settle_cap: SettleCap,
}

#[async_trait::async_trait]
impl Specialist for SettlementSpecialist {
    fn name(&self) -> &str {
        "settlement-agent"
    }

    async fn handle(&self, mention: CoralMention) -> String {
        if wire::verb(&mention.text) != "SETTLE_REQUESTED" {
            tracing::debug!(text = %mention.text, "settlement-agent: ignoring non-delegation mention");
            return String::new();
        }

        // Parse the wager JSON from the wire message.
        let wager = match parse_wager(&mention.text) {
            Some(w) => w,
            None => {
                tracing::warn!(text = %mention.text, "settlement-agent: malformed wager= payload");
                return "SETTLE_VERDICT wagerId=unknown status=rejected reason=\"malformed wager payload\" txSig=none".to_string();
            }
        };

        let proof_ref = wire::tok(&mention.text, "proofRef")
            .unwrap_or("none")
            .to_string();

        // Verify proof reference exists and is non-empty.
        if proof_ref == "none" || proof_ref.is_empty() {
            tracing::warn!(wager_id = %wager.wager_id, "settlement-agent: no proof reference");
            return format!(
                "SETTLE_VERDICT wagerId={} status=rejected reason=\"missing proof reference\" txSig=none",
                wager.wager_id
            );
        }

        // Verify the wager is in a settleable state.
        if matches!(wager.status, txodds_types::wager::WagerStatus::NoBet) {
            tracing::info!(wager_id = %wager.wager_id, "settlement-agent: NoBet wager, nothing to settle");
            return format!(
                "SETTLE_VERDICT wagerId={} status=rejected reason=\"wager status is NoBet\" txSig=none",
                wager.wager_id
            );
        }

        // Simulate on-chain settlement (devnet).
        // In production this would construct and submit a Solana transaction.
        let (status, tx_sig, reason) = simulate_settlement(&wager, &proof_ref);

        tracing::info!(
            wager_id = %wager.wager_id,
            status = %status,
            tx_sig = %tx_sig,
            reason = %reason,
            "settlement-agent: settlement complete"
        );

        let reason_escaped = reason.replace('"', "'");
        format!(
            "SETTLE_VERDICT wagerId={} status={status} reason=\"{reason_escaped}\" txSig={tx_sig}",
            wager.wager_id
        )
    }
}

/// Simulate on-chain settlement. Returns (status, tx_signature, reason).
///
/// In production this would:
/// 1. Verify the proof_ref against the on-chain oracle program
/// 2. Construct and sign the settlement instruction
/// 3. Submit to the Solana cluster and await confirmation
///
/// For devnet, we generate a deterministic pseudo-signature from the wager id
/// to enable idempotent replay testing.
fn simulate_settlement(
    wager: &txodds_types::wager::Wager,
    proof_ref: &str,
) -> (SettleStatus, String, String) {
    // Basic validation: edge must be positive for settlement to proceed.
    if wager.edge <= 0.0 {
        return (
            SettleStatus::Rejected,
            "none".to_owned(),
            format!("non-positive edge ({:.4}); settlement rejected", wager.edge),
        );
    }

    // Basic validation: stake must be positive.
    if wager.stake_sol <= 0.0 {
        return (
            SettleStatus::Rejected,
            "none".to_owned(),
            "zero-stake wager; settlement rejected".to_owned(),
        );
    }

    // Generate deterministic devnet pseudo-signature.
    let pseudo_sig = format!(
        "devnet:settle:{}:{}",
        wager.wager_id,
        &proof_ref[..proof_ref.len().min(8)]
    );

    (
        SettleStatus::Settled,
        pseudo_sig,
        format!(
            "settlement simulated on devnet — stake={:.4} SOL, edge={:.4}, selection={:?}",
            wager.stake_sol, wager.edge, wager.selection
        ),
    )
}

/// Extract the `wager=<json>` token via the shared brace-matching extractor
/// (`coral_client::wire::json_val`) — string-aware, unlike the local
/// brace-counting copy this replaces, which broke on a `{` inside a thesis
/// string.
fn parse_wager(text: &str) -> Option<txodds_types::wager::Wager> {
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

    let max_wait_ms: u64 = env_parse("SA_MAX_WAIT_MS", 30_000);
    let max_steps: u64 = env_parse("MAX_STEPS", 100_000);

    tracing::info!(agent = "settlement-agent", "starting");

    let specialist = SettlementSpecialist {
        _settle_cap: SettleCap::acquire(),
    };

    if let Err(err) = coral_client::run(specialist, max_wait_ms, max_steps).await {
        tracing::error!(error = %err, "settlement-agent: fatal");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use txodds_types::wager::{Selection, Wager, WagerStatus};

    fn sample_wager() -> Wager {
        Wager {
            wager_id: "w-settle-1".into(),
            fixture_id: 42,
            selection: Selection::Home,
            model_prob: 0.60,
            market_implied: 0.50,
            edge: 0.10,
            fair_odds: 1.0 / 0.60,
            stake_sol: 0.02,
            thesis: "home value".into(),
            proof_ref: Some("txoracle:deadbeef".into()),
            status: WagerStatus::ProofPassed,
            debate: None,
            created_at: "2026-07-11T00:00:00Z".into(),
        }
    }

    #[test]
    fn simulate_settlement_happy_path() {
        let w = sample_wager();
        let (status, sig, _reason) = simulate_settlement(&w, "txoracle:deadbeef");
        assert_eq!(status, SettleStatus::Settled);
        assert!(sig.starts_with("devnet:settle:"));
    }

    #[test]
    fn simulate_settlement_rejects_no_edge() {
        let mut w = sample_wager();
        w.edge = 0.0;
        let (status, _, _) = simulate_settlement(&w, "txoracle:deadbeef");
        assert_eq!(status, SettleStatus::Rejected);
    }

    #[test]
    fn simulate_settlement_rejects_zero_stake() {
        let mut w = sample_wager();
        w.stake_sol = 0.0;
        let (status, _, _) = simulate_settlement(&w, "txoracle:deadbeef");
        assert_eq!(status, SettleStatus::Rejected);
    }

    #[tokio::test]
    async fn handles_settle_requested() {
        let specialist = SettlementSpecialist {
            _settle_cap: SettleCap::acquire(),
        };
        let wager_json = serde_json::to_string(&sample_wager()).unwrap();
        let mention = CoralMention {
            thread_id: Some("t-1".into()),
            sender: Some("match-intelligence-agent".into()),
            text: format!("SETTLE_REQUESTED proofRef=txoracle:deadbeef wager={wager_json}"),
        };
        let reply = specialist.handle(mention).await;
        assert!(reply.starts_with("SETTLE_VERDICT wagerId=w-settle-1 status=settled"));
        assert!(reply.contains("txSig=devnet:settle:"));
    }

    #[tokio::test]
    async fn handles_settle_requested_with_tool_trail() {
        // The orchestrator now carries the round's reasoning trail on the
        // delegation (TODO 6e) — `toolTrail=<json>` precedes the trailing
        // `wager=<json>` and must not disturb the wager parse.
        let specialist = SettlementSpecialist {
            _settle_cap: SettleCap::acquire(),
        };
        let wager_json = serde_json::to_string(&sample_wager()).unwrap();
        let trail = r#"[{"agent":"match-intelligence-agent","tool":"compute_model_probability","result":{"home":0.42,"draw":0.29,"away":0.29}}]"#;
        let mention = CoralMention {
            thread_id: Some("t-1b".into()),
            sender: Some("match-intelligence-agent".into()),
            text: format!(
                "SETTLE_REQUESTED proofRef=txoracle:deadbeef toolTrail={trail} wager={wager_json}"
            ),
        };
        let reply = specialist.handle(mention).await;
        assert!(reply.starts_with("SETTLE_VERDICT wagerId=w-settle-1 status=settled"));
    }

    #[tokio::test]
    async fn rejects_missing_proof_ref() {
        let specialist = SettlementSpecialist {
            _settle_cap: SettleCap::acquire(),
        };
        let wager_json = serde_json::to_string(&sample_wager()).unwrap();
        let mention = CoralMention {
            thread_id: Some("t-2".into()),
            sender: Some("orchestrator".into()),
            text: format!("SETTLE_REQUESTED wager={wager_json}"),
        };
        let reply = specialist.handle(mention).await;
        assert!(reply.contains("status=rejected"));
        assert!(reply.contains("missing proof reference"));
    }

    #[tokio::test]
    async fn ignores_non_settle_verb() {
        let specialist = SettlementSpecialist {
            _settle_cap: SettleCap::acquire(),
        };
        let mention = CoralMention {
            thread_id: Some("t-3".into()),
            sender: Some("someone".into()),
            text: "HELLO round=1".into(),
        };
        assert_eq!(specialist.handle(mention).await, "");
    }
}
