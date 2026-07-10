//! Wager Authority — the Rust-side safety spine over agent proposals.
//!
//! Specialist agents (Python) may *propose* a wager with a model probability,
//! market-implied probability, and a thesis. They may **not** decide the stake,
//! bypass the proof gate, or exceed the devnet spend cap. This module is the
//! single choke point that:
//!
//! 1. recomputes the edge from first principles (never trusts the agent's edge),
//! 2. rejects any wager whose edge is below the configured minimum → `NoBet`,
//! 3. sizes the stake with Kelly and clamps it to `max_devnet_spend_sol`,
//! 4. refuses to advance past `ProofPassed` unless a `proof_ref` is present.
//!
//! Everything here is synchronous and pure so it is trivially testable; the
//! math lives in `txodds_types::{kelly_fraction, implied_probability}`.

use txodds_types::{implied_probability, kelly_fraction, Wager, WagerStatus};

/// Policy inputs the Authority enforces. Sourced from [`crate::config::AppConfig`].
#[derive(Debug, Clone, Copy)]
pub struct AuthorityPolicy {
    /// Minimum edge (`model_prob − market_implied`) required to bet at all.
    pub min_edge: f64,
    /// Fraction of a full Kelly stake to use (e.g. 0.25 = quarter Kelly).
    pub kelly_fraction_multiplier: f64,
    /// Bankroll in SOL that the Kelly fraction is applied against.
    pub bankroll_sol: f64,
    /// Hard cap on any single stake, in SOL. Never exceeded.
    pub max_devnet_spend_sol: f64,
}

impl AuthorityPolicy {
    /// Conservative defaults derived from the app config's devnet cap.
    #[must_use]
    pub fn from_max_spend(max_devnet_spend_sol: f64) -> Self {
        Self {
            min_edge: 0.02,
            kelly_fraction_multiplier: 0.25,
            // Treat the devnet cap as the notional bankroll so a full-Kelly bet
            // can never on its own exceed the cap before the hard clamp.
            bankroll_sol: max_devnet_spend_sol.max(0.0),
            max_devnet_spend_sol: max_devnet_spend_sol.max(0.0),
        }
    }
}

/// The Authority's ruling on a proposed wager.
#[derive(Debug, Clone)]
pub struct AuthorityRuling {
    /// The wager after edge recomputation, sizing, and clamping.
    pub wager: Wager,
    /// Human-readable justification, shown in the transcript.
    pub reason: String,
}

/// Adjudicate a proposed wager against policy.
///
/// The incoming `wager.model_prob` is trusted (it is the debate's output), but
/// the edge, fair odds, stake, and status are all recomputed here. Market odds
/// are supplied separately so the agent cannot lie about the implied price.
///
/// Returns a ruling whose `wager.status` is one of:
/// - [`WagerStatus::NoBet`] — edge below `min_edge`, or degenerate probability;
/// - [`WagerStatus::ProofPassed`] — positive edge, staked, proof present;
/// - [`WagerStatus::Debated`] — positive edge, staked, but proof still missing.
#[must_use]
pub fn adjudicate(mut wager: Wager, market_decimal_odds: f64, policy: AuthorityPolicy) -> AuthorityRuling {
    // 1. Recompute market-implied probability from the odds we were given.
    let Some(market_implied) = implied_probability(market_decimal_odds) else {
        wager.stake_sol = 0.0;
        wager.status = WagerStatus::NoBet;
        wager.market_implied = 0.0;
        wager.edge = 0.0;
        return AuthorityRuling {
            reason: format!("invalid market odds {market_decimal_odds}; no bet"),
            wager,
        };
    };
    wager.market_implied = market_implied;

    // 2. Recompute edge and fair odds from the model probability only.
    if !(wager.model_prob > 0.0 && wager.model_prob < 1.0) {
        wager.stake_sol = 0.0;
        wager.edge = 0.0;
        wager.status = WagerStatus::NoBet;
        return AuthorityRuling {
            reason: format!("degenerate model probability {}; no bet", wager.model_prob),
            wager,
        };
    }
    wager.edge = wager.model_prob - market_implied;
    wager.fair_odds = 1.0 / wager.model_prob;

    // 3. Gate on minimum edge.
    if wager.edge <= policy.min_edge {
        wager.stake_sol = 0.0;
        wager.status = WagerStatus::NoBet;
        return AuthorityRuling {
            reason: format!(
                "edge {:.4} ≤ min {:.4}; no positive-value bet",
                wager.edge, policy.min_edge
            ),
            wager,
        };
    }

    // 4. Size with fractional Kelly, then clamp to the hard devnet cap.
    //    Kelly uses the *market* odds we are actually offered — betting at our
    //    own fair odds would yield zero edge by construction and stake nothing.
    let full_kelly = kelly_fraction(wager.model_prob, market_decimal_odds);
    let sized = full_kelly * policy.kelly_fraction_multiplier * policy.bankroll_sol;
    let staked = sized.clamp(0.0, policy.max_devnet_spend_sol);
    wager.stake_sol = staked;

    if staked <= 0.0 {
        wager.status = WagerStatus::NoBet;
        return AuthorityRuling {
            reason: "kelly stake rounded to zero; no bet".to_string(),
            wager,
        };
    }

    // 5. Proof gate: only ProofPassed if the attestation is present.
    let clamped = (sized - staked).abs() > f64::EPSILON;
    wager.status = if wager.proof_ref.is_some() {
        WagerStatus::ProofPassed
    } else {
        WagerStatus::Debated
    };
    let reason = format!(
        "edge {:.4} > min {:.4}; {}kelly {:.4} → stake {:.5} SOL{} (status {:?})",
        wager.edge,
        policy.min_edge,
        format_args!("{}× ", policy.kelly_fraction_multiplier),
        full_kelly,
        staked,
        if clamped { " [clamped to cap]" } else { "" },
        wager.status,
    );

    AuthorityRuling { wager, reason }
}

#[cfg(test)]
mod tests {
    use super::*;
    use txodds_types::Selection;

    fn base_wager(model_prob: f64) -> Wager {
        Wager {
            wager_id: "w1".into(),
            fixture_id: 1,
            selection: Selection::Home,
            model_prob,
            market_implied: 0.0,
            edge: 0.0,
            fair_odds: 0.0,
            stake_sol: 0.0,
            thesis: "test".into(),
            proof_ref: None,
            status: WagerStatus::Proposed,
            debate: None,
            created_at: "2026-01-01T00:00:00.000Z".into(),
        }
    }

    fn policy() -> AuthorityPolicy {
        AuthorityPolicy {
            min_edge: 0.02,
            kelly_fraction_multiplier: 0.25,
            bankroll_sol: 1.0,
            max_devnet_spend_sol: 0.05,
        }
    }

    #[test]
    fn no_edge_is_no_bet() {
        // model 0.5 vs market 0.5 (odds 2.0) → edge 0.
        let ruling = adjudicate(base_wager(0.5), 2.0, policy());
        assert_eq!(ruling.wager.status, WagerStatus::NoBet);
        assert_eq!(ruling.wager.stake_sol, 0.0);
    }

    #[test]
    fn positive_edge_sizes_and_gates_on_proof() {
        // model 0.6 vs market 0.5 (odds 2.0) → edge 0.1, positive.
        let ruling = adjudicate(base_wager(0.6), 2.0, policy());
        assert!(ruling.wager.edge > 0.0);
        assert!(ruling.wager.stake_sol > 0.0);
        // No proof_ref → cannot pass proof gate.
        assert_eq!(ruling.wager.status, WagerStatus::Debated);
    }

    #[test]
    fn proof_ref_allows_proof_passed() {
        let mut w = base_wager(0.6);
        w.proof_ref = Some("sha256:deadbeef".into());
        let ruling = adjudicate(w, 2.0, policy());
        assert_eq!(ruling.wager.status, WagerStatus::ProofPassed);
    }

    #[test]
    fn stake_never_exceeds_cap() {
        // Huge bankroll: Kelly would blow past the cap, but clamp holds.
        let p = AuthorityPolicy {
            bankroll_sol: 1000.0,
            ..policy()
        };
        let ruling = adjudicate(base_wager(0.9), 2.0, p);
        assert!(ruling.wager.stake_sol <= 0.05 + f64::EPSILON);
    }

    #[test]
    fn invalid_odds_is_no_bet() {
        let ruling = adjudicate(base_wager(0.6), 1.0, policy());
        assert_eq!(ruling.wager.status, WagerStatus::NoBet);
    }
}
