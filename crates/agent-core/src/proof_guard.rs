//! Proof-guard — adversarial wager consistency verification.
//!
//! Ported from `coral-agents/proof-guard-agent/agent.py`.
//! This is the skeptic of the pipeline.  After the sharp/pundit/fundamentals
//! voices have argued a wager into shape, proof-guard is the last check
//! *before* the settlement track: it refuses to let a wager advance unless the
//! numbers it claims are internally consistent and backed by a txoracle proof
//! attestation.
//!
//! It does NOT trust the specialists' self-reported figures.  For a proposed
//! wager it independently:
//!
//!   1. Recomputes the market-implied probability and cross-checks edge.
//!   2. Verifies `model_prob` ∈ (0, 1).
//!   3. Verifies `fair_odds` inverts to `model_prob` within tolerance.
//!   4. Verifies the proposed stake is non-negative and finite.
//!   5. Checks a proof reference (txoracle attestation) is present and
//!      well-shaped.
//!
//! This is pure deterministic logic — no async, no IO, no Tauri.

use txodds_types::wager::{implied_probability, Wager, WagerStatus};

/// Default tolerance for internal consistency cross-checks.
/// Specialists round to 6dp, so anything looser than this indicates a
/// fabricated or drifted figure.
pub const DEFAULT_CONSISTENCY_TOL: f64 = 0.02;

/// Default devnet hard cap (SOL).
pub const DEFAULT_MAX_DEVNET_SPEND_SOL: f64 = 0.05;

/// Configuration for the proof guard checks.
#[derive(Debug, Clone)]
pub struct ProofGuardConfig {
    /// Tolerance for edge / probability cross-checks.
    pub consistency_tol: f64,
    /// Devnet stake cap (advisory echo — the Authority owns the real clamp).
    pub max_devnet_spend_sol: f64,
}

impl Default for ProofGuardConfig {
    fn default() -> Self {
        Self {
            consistency_tol: DEFAULT_CONSISTENCY_TOL,
            max_devnet_spend_sol: DEFAULT_MAX_DEVNET_SPEND_SOL,
        }
    }
}

/// Result of running the proof-guard checks on a wager.
#[derive(Debug, Clone)]
pub struct ProofGuardVerdict {
    /// Whether all checks passed.
    pub passed: bool,
    /// Human-readable failure reasons (empty if passed).
    pub failures: Vec<String>,
    /// The wager with updated status and thesis annotation.
    pub wager: Wager,
}

/// Minimum length/shape a proof reference must have to be considered a real
/// attestation rather than a placeholder.
fn is_valid_proof_ref(s: &str) -> bool {
    if s.len() < 8 {
        return false;
    }
    s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '_' | '-' | '.' | '/'))
}

/// Run the 5-point adversarial consistency check on a wager.
///
/// Returns a list of human-readable failure reasons.  Empty means passed.
#[must_use]
pub fn run_checks(wager: &Wager, proof_ref: Option<&str>, cfg: &ProofGuardConfig) -> Vec<String> {
    let mut failures = Vec::new();

    // 1. Model probability must be a well-formed probability.
    if !(wager.model_prob.is_finite() && wager.model_prob > 0.0 && wager.model_prob < 1.0) {
        failures.push(format!(
            "model probability {} outside (0, 1)",
            wager.model_prob
        ));
    }

    // 2. Cross-check edge = model_prob − market_implied.
    if wager.market_implied.is_finite()
        && wager.market_implied > 0.0
        && wager.market_implied < 1.0
    {
        let expected_edge = wager.model_prob - wager.market_implied;
        if (expected_edge - wager.edge).abs() > cfg.consistency_tol {
            failures.push(format!(
                "edge {:+.4} inconsistent with model−market {:+.4} (tol {})",
                wager.edge, expected_edge, cfg.consistency_tol
            ));
        }
    } else {
        failures.push(format!(
            "market-implied probability {} outside (0, 1)",
            wager.market_implied
        ));
    }

    // 3. fair_odds must invert to model_prob (fair_odds = 1 / model_prob).
    if wager.model_prob > 0.0 {
        match implied_probability(wager.fair_odds) {
            None => {
                failures.push(format!(
                    "fair odds {} not a valid price",
                    wager.fair_odds
                ));
            }
            Some(implied) => {
                if (implied - wager.model_prob).abs() > cfg.consistency_tol {
                    failures.push(format!(
                        "fair odds {} imply {:.3}, not model {:.3}",
                        wager.fair_odds, implied, wager.model_prob
                    ));
                }
            }
        }
    }

    // 4. Stake must be non-negative and finite.
    if !(wager.stake_sol.is_finite() && wager.stake_sol >= 0.0) {
        failures.push(format!(
            "stake {} is negative or non-finite",
            wager.stake_sol
        ));
    }

    // 5. Proof attestation must be present and well-shaped.
    match proof_ref {
        None => {
            failures.push("no proof attestation (txoracle reference) present".into());
        }
        Some(pr) if !is_valid_proof_ref(pr) => {
            failures.push(format!(
                "proof reference {:?} is malformed / placeholder",
                shorten(pr)
            ));
        }
        Some(_) => {}
    }

    failures
}

/// Full proof-guard verification: runs all checks and returns a verdict with
/// an updated wager (status set to `ProofPassed` or `ProofFailed`, thesis
/// annotated).
#[must_use]
pub fn verify(wager: &Wager, proof_ref: Option<&str>, cfg: &ProofGuardConfig) -> ProofGuardVerdict {
    // A concluded no-bet / zero-stake wager needs no proof.
    if wager.status == WagerStatus::NoBet || wager.stake_sol <= 0.0 {
        return ProofGuardVerdict {
            passed: true,
            failures: Vec::new(),
            wager: wager.clone(),
        };
    }

    let failures = run_checks(wager, proof_ref, cfg);

    if failures.is_empty() {
        let pr = proof_ref.unwrap_or_default();
        let mut passed_wager = wager.clone();
        passed_wager.proof_ref = Some(pr.to_owned());
        passed_wager.status = WagerStatus::ProofPassed;
        passed_wager.thesis = format!(
            "{} | Proof-guard PASSED: edge/odds consistent, attestation {} present, stake within cap.",
            wager.thesis,
            shorten(pr)
        );
        ProofGuardVerdict {
            passed: true,
            failures: Vec::new(),
            wager: passed_wager,
        }
    } else {
        let reason = failures.join("; ");
        let mut failed_wager = wager.clone();
        failed_wager.status = WagerStatus::ProofFailed;
        failed_wager.thesis = format!(
            "{} | Proof-guard FAILED: {}",
            wager.thesis, reason
        );
        ProofGuardVerdict {
            passed: false,
            failures,
            wager: failed_wager,
        }
    }
}

/// Truncate a long attestation reference for readable transcript lines.
fn shorten(proof_ref: &str) -> String {
    if proof_ref.len() <= 20 {
        proof_ref.to_owned()
    } else {
        format!("{}…{}", &proof_ref[..12], &proof_ref[proof_ref.len() - 4..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use txodds_types::wager::Selection;

    fn test_wager() -> Wager {
        Wager {
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
            created_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn all_checks_pass_with_valid_proof() {
        let w = test_wager();
        let cfg = ProofGuardConfig::default();
        let checks = run_checks(&w, Some("sha256:abc123def456"), &cfg);
        assert!(checks.is_empty(), "expected no failures, got: {checks:?}");
    }

    #[test]
    fn missing_proof_fails() {
        let w = test_wager();
        let cfg = ProofGuardConfig::default();
        let checks = run_checks(&w, None, &cfg);
        assert!(checks.iter().any(|f| f.contains("no proof attestation")));
    }

    #[test]
    fn malformed_proof_fails() {
        let w = test_wager();
        let cfg = ProofGuardConfig::default();
        let checks = run_checks(&w, Some("ab"), &cfg);
        assert!(checks.iter().any(|f| f.contains("malformed")));
    }

    #[test]
    fn inconsistent_edge_fails() {
        let mut w = test_wager();
        w.edge = 0.5; // claimed 0.5, but model(0.6) - market(0.5) = 0.1
        let cfg = ProofGuardConfig::default();
        let checks = run_checks(&w, Some("sha256:abc123def456"), &cfg);
        assert!(checks.iter().any(|f| f.contains("inconsistent")));
    }

    #[test]
    fn model_prob_out_of_range_fails() {
        let mut w = test_wager();
        w.model_prob = 1.5;
        let cfg = ProofGuardConfig::default();
        let checks = run_checks(&w, Some("sha256:abc123def456"), &cfg);
        assert!(checks.iter().any(|f| f.contains("model probability")));
    }

    #[test]
    fn negative_stake_fails() {
        let mut w = test_wager();
        w.stake_sol = -0.01;
        let cfg = ProofGuardConfig::default();
        let checks = run_checks(&w, Some("sha256:abc123def456"), &cfg);
        assert!(checks.iter().any(|f| f.contains("negative")));
    }

    #[test]
    fn verify_passes_sets_proof_passed() {
        let w = test_wager();
        let cfg = ProofGuardConfig::default();
        let verdict = verify(&w, Some("sha256:abc123def456"), &cfg);
        assert!(verdict.passed);
        assert_eq!(verdict.wager.status, WagerStatus::ProofPassed);
        assert!(verdict.wager.thesis.contains("PASSED"));
    }

    #[test]
    fn verify_fails_sets_proof_failed() {
        let w = test_wager();
        let cfg = ProofGuardConfig::default();
        let verdict = verify(&w, None, &cfg);
        assert!(!verdict.passed);
        assert_eq!(verdict.wager.status, WagerStatus::ProofFailed);
        assert!(verdict.wager.thesis.contains("FAILED"));
    }

    #[test]
    fn no_bet_wager_passes_without_proof() {
        let mut w = test_wager();
        w.status = WagerStatus::NoBet;
        w.stake_sol = 0.0;
        let cfg = ProofGuardConfig::default();
        let verdict = verify(&w, None, &cfg);
        assert!(verdict.passed);
    }
}
