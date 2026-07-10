//! Deterministic market feature extraction.

use serde::{Deserialize, Serialize};
use txodds_types::{TxLineEvent, TxLineEventKind, ValidationSimulationStatus};

/// Extracted market features used by the policy layer to choose an action.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketFeatures {
    /// `TxLINE` fixture ID the features relate to.
    pub fixture_id: u64,
    /// Debug representation of the event kind.
    pub kind: String,
    /// Whether a score payload was present on the event.
    pub has_score: bool,
    /// Whether odds quotes were present on the event.
    pub has_odds: bool,
    /// Highest implied probability across all odds quotes (if any).
    pub best_implied_probability: Option<f64>,
    /// Whether a proof payload was attached.
    pub proof_present: bool,
    /// Whether a Merkle root was present in the proof payload.
    pub root_present: bool,
    /// Whether the `TxOracle` simulation passed.
    pub txoracle_passed: bool,
    /// Composite severity score in `[0.0, 1.0]`.
    pub severity_score: f64,
    /// Composite actionability score in `[0.0, 1.0]`.
    pub actionability_score: f64,
    /// Human-readable reasons contributing to the scores.
    pub reasons: Vec<String>,
}

/// Extract deterministic market features from a raw `TxLINE` event.
#[must_use]
pub fn derive_features(event: &TxLineEvent) -> MarketFeatures {
    let mut features = MarketFeatures {
        fixture_id: event.fixture_id,
        kind: format!("{:?}", event.kind),
        has_score: event.score.is_some(),
        has_odds: event
            .odds
            .as_ref()
            .is_some_and(|items| !items.is_empty()),
        best_implied_probability: best_implied_probability(event),
        proof_present: event
            .proof
            .as_ref()
            .is_some_and(|proof| proof.proof_present),
        root_present: event
            .proof
            .as_ref()
            .is_some_and(|proof| proof.root_present),
        txoracle_passed: event
            .proof
            .as_ref()
            .is_some_and(|proof| matches!(proof.simulation_status, ValidationSimulationStatus::Passed)),
        ..MarketFeatures::default()
    };

    match &event.kind {
        TxLineEventKind::Goal => {
            features.severity_score += 0.80;
            features
                .reasons
                .push("goal changes match state".to_string());
        }
        TxLineEventKind::RedCard => {
            features.severity_score += 0.78;
            features
                .reasons
                .push("red card can reprice market".to_string());
        }
        TxLineEventKind::FinalWhistle => {
            features.severity_score += 0.72;
            features
                .reasons
                .push("final whistle can trigger resolution".to_string());
        }
        TxLineEventKind::OddsMove | TxLineEventKind::OddsUpdate => {
            features.severity_score += 0.64;
            features.reasons.push("odds update observed".to_string());
        }
        TxLineEventKind::ProofReceived => {
            features.severity_score += 0.70;
            features.reasons.push("proof receipt arrived".to_string());
        }
        _ => {
            features.severity_score += 0.35;
            features.reasons.push("context update observed".to_string());
        }
    }

    if features.has_odds {
        features.actionability_score += 0.20;
    }
    if features.has_score {
        features.actionability_score += 0.20;
    }
    if features.proof_present {
        features.actionability_score += 0.20;
    }
    if features.root_present {
        features.actionability_score += 0.20;
    }
    if features.txoracle_passed {
        features.actionability_score += 0.20;
    }

    features.severity_score = features.severity_score.min(1.0);
    features.actionability_score = features.actionability_score.min(1.0);
    features
}

fn best_implied_probability(event: &TxLineEvent) -> Option<f64> {
    event
        .odds
        .as_ref()?
        .iter()
        .map(|quote| quote.implied_probability)
        .filter(|value| value.is_finite())
        .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
}
