//! Wager domain types: the object that settlement finally acts on.
//!
//! A [`Wager`] is the concrete obligation produced by the Tier-3 adversarial
//! debate between the specialist agents. It captures the *reasoning* (model vs
//! market), the *edge* that justifies a bet, the *Kelly-sized* stake, and the
//! *proof attestation* that gates settlement. Nothing here has async or chain
//! access — this crate remains pure data, mirrored by the Python framework's
//! Pydantic `Wager` model and the frontend `ui/types.ts` contract.
//!
//! Lifecycle: `Proposed → Debated → ProofPassed → Signed → Settled`
//! (or the honest terminal states `NoBet` / `ProofFailed` / `Refunded`).

use serde::{Deserialize, Serialize};

/// The outcome a wager backs. Mirrors the 1X2 market shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Selection {
    Home,
    Draw,
    Away,
}

/// Lifecycle state of a wager, from proposal through settlement.
///
/// The debate coordinator advances `Proposed → Debated`; proof-guard advances
/// `Debated → ProofPassed` (or `ProofFailed`); the settlement agent + user
/// signature advance `ProofPassed → Signed → Settled`. `NoBet` is the honest
/// null result when the edge is insufficient.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WagerStatus {
    /// Initial thesis synthesized, not yet cross-examined.
    Proposed,
    /// Survived the adversarial debate rounds.
    Debated,
    /// Debate concluded that no positive-edge bet is justified.
    NoBet,
    /// Proof-guard verified every input datum used in the thesis.
    ProofPassed,
    /// Proof-guard could not verify one or more inputs; wager is vetoed.
    ProofFailed,
    /// User signed the escrow release; funds committed on devnet.
    Signed,
    /// Result verified at final whistle; escrow released to the winner.
    Settled,
    /// Result verified against the selection; stake returned.
    Refunded,
}

/// A single specialist's contribution to a debate round.
///
/// Rendered verbatim in the Console transcript so the argument is visible.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebateContribution {
    /// Agent id (e.g. `match-intelligence-agent`).
    pub agent_id: String,
    /// Which debate round this belongs to (0 = trigger).
    pub round: u64,
    /// `analysis` | `signal` | `narrative` | `challenge` | `endorse` | `arbitrate`.
    pub kind: String,
    /// The agent's stance summary, shown in the transcript.
    pub summary: String,
    /// This agent's probability estimate for the selection, if it produced one.
    #[serde(default)]
    pub prob: Option<f64>,
    /// This agent's confidence in its own contribution, 0.0–1.0.
    #[serde(default)]
    pub confidence: Option<f64>,
    /// Agent ids this contribution challenges or endorses, if any.
    #[serde(default)]
    pub targets: Vec<String>,
}

/// The full transcript of the adversarial debate that produced a wager.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebateSummary {
    /// How many rounds ran before convergence or the round cap.
    pub rounds: u64,
    /// Whether the agents converged (`true`) or hit the round cap (`false`).
    pub converged: bool,
    /// Every contribution, in order, for full auditability.
    pub contributions: Vec<DebateContribution>,
}

/// A proof-verified, Kelly-sized wager — the object settlement acts on.
///
/// `model_prob` comes from the fundamental agent, `market_implied` from the
/// sharp-movement detector's odds conversion, and `edge = model_prob -
/// market_implied`. A positive edge above the configured minimum is required
/// for `stake_sol` to be non-zero; otherwise the wager terminates as
/// [`WagerStatus::NoBet`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Wager {
    pub wager_id: String,
    pub fixture_id: u64,
    pub selection: Selection,
    /// Agents' fair probability for the selection, 0.0–1.0.
    pub model_prob: f64,
    /// Market-implied probability derived from decimal odds, 0.0–1.0.
    pub market_implied: f64,
    /// `model_prob - market_implied`. Positive = value.
    pub edge: f64,
    /// Fair decimal odds implied by `model_prob` (`1.0 / model_prob`).
    pub fair_odds: f64,
    /// Kelly-sized stake in SOL, clamped by the Rust Authority to
    /// `max_devnet_spend_sol`. Zero when [`WagerStatus::NoBet`].
    pub stake_sol: f64,
    /// The surviving debate conclusion in plain language.
    pub thesis: String,
    /// Proof-guard attestation reference (e.g. `sha256:…`) once verified.
    #[serde(default)]
    pub proof_ref: Option<String>,
    /// Current lifecycle state.
    pub status: WagerStatus,
    /// The debate that produced this wager, for the audit panel.
    #[serde(default)]
    pub debate: Option<DebateSummary>,
    /// Creation timestamp (RFC3339, ms precision).
    pub created_at: String,
}

impl Wager {
    /// True when the wager carries a positive edge above `min_edge` and thus
    /// justifies a non-zero stake. Pure helper — no policy enforcement here.
    #[must_use]
    pub fn has_value(&self, min_edge: f64) -> bool {
        self.edge > min_edge && self.model_prob > 0.0 && self.model_prob < 1.0
    }
}

/// Kelly stake fraction for a single binary outcome.
///
/// `f* = (b·p − q) / b` where `b` = net decimal odds (`fair_odds − 1`),
/// `p` = win probability, `q = 1 − p`. Returns a fraction of bankroll in
/// `[0.0, 1.0]`; negative or non-finite results are clamped to `0.0` (no bet).
///
/// This is pure math and lives here so both the Authority API and tests share
/// one definition. Stake *caps* (bankroll, `max_devnet_spend_sol`) are applied
/// by the Rust Authority, never here.
#[must_use]
pub fn kelly_fraction(model_prob: f64, fair_odds: f64) -> f64 {
    if !(model_prob.is_finite() && fair_odds.is_finite()) {
        return 0.0;
    }
    if model_prob <= 0.0 || model_prob >= 1.0 || fair_odds <= 1.0 {
        return 0.0;
    }
    let b = fair_odds - 1.0;
    let q = 1.0 - model_prob;
    let f = (b * model_prob - q) / b;
    f.clamp(0.0, 1.0)
}

/// Convert decimal odds to implied probability, ignoring the overround.
///
/// `implied = 1 / decimal_odds`. Returns `None` for non-finite or non-positive
/// odds. The bookmaker margin (overround) is left for the caller to normalize
/// across the full 1X2 set if desired.
#[must_use]
pub fn implied_probability(decimal_odds: f64) -> Option<f64> {
    if decimal_odds.is_finite() && decimal_odds > 1.0 {
        Some(1.0 / decimal_odds)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kelly_zero_when_no_edge() {
        // Fair coin at even money: p=0.5, odds=2.0 → edge zero → f*=0.
        assert_eq!(kelly_fraction(0.5, 2.0), 0.0);
    }

    #[test]
    fn kelly_positive_with_edge() {
        // p=0.6 at even money (2.0) → f* = (1·0.6 − 0.4)/1 = 0.2.
        let f = kelly_fraction(0.6, 2.0);
        assert!((f - 0.2).abs() < 1e-9, "expected 0.2, got {f}");
    }

    #[test]
    fn kelly_clamped_for_bad_inputs() {
        assert_eq!(kelly_fraction(1.5, 2.0), 0.0);
        assert_eq!(kelly_fraction(0.6, 0.9), 0.0);
        assert_eq!(kelly_fraction(f64::NAN, 2.0), 0.0);
    }

    #[test]
    fn implied_probability_basic() {
        assert_eq!(implied_probability(2.0), Some(0.5));
        assert_eq!(implied_probability(4.0), Some(0.25));
        assert_eq!(implied_probability(1.0), None);
        assert_eq!(implied_probability(0.0), None);
    }

    #[test]
    fn has_value_requires_edge_above_min() {
        let w = Wager {
            wager_id: "w1".into(),
            fixture_id: 7,
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
        };
        assert!(w.has_value(0.05));
        assert!(!w.has_value(0.2));
    }
}
