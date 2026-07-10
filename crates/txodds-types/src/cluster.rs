//! Cluster and product-track enumerations.
//!
//! These are the top-level discriminants used across every module: which Solana
//! cluster is active, and which product track the current run belongs to.

use serde::{Deserialize, Serialize};

/// Solana clusters supported by the desktop command surface.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Cluster {
    Devnet,
    Mainnet,
}

/// Product track selected in the UI and recorded on every run.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrackMode {
    Settlement,
    Trading,
    Fan,
}

impl std::fmt::Display for TrackMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Settlement => "settlement",
            Self::Trading => "trading",
            Self::Fan => "fan",
        })
    }
}
