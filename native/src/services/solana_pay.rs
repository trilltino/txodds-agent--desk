//! Solana Pay payment intent types and construction.
//!
//! `SolanaPayIntent` represents a pending or settled Solana Pay request that the
//! agent creates when a user opts into on-chain settlement. The record is
//! persisted to the SQLite ledger via `LedgerStore::upsert_payment_intent` and
//! used by the chain service to poll for confirmation.
//!
//! rig-venice ROADMAP.md Phase 7, item 3: this module used to be data-only —
//! the type existed, nothing built a payment URL or a reference key, and no
//! Tauri command exposed any of it (see `commands::payments`). This is the
//! wallet-approval-before-settlement moment the roadmap describes: the user
//! is shown this URL/reference and must approve it with their own wallet —
//! nothing here can move funds by itself, it only *requests* a transfer that
//! a human's wallet software has to sign.

use serde::{Deserialize, Serialize};

/// Lifecycle status of a Solana Pay payment intent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolanaPayStatus {
    /// Created but not yet confirmed on-chain.
    Pending,
    /// Transaction confirmed on the Solana cluster.
    Confirmed,
    /// Transaction failed or was rejected.
    Failed,
    /// Intent was not fulfilled within the TTL window.
    Expired,
}

/// A Solana Pay payment intent persisted by the ledger.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SolanaPayIntent {
    /// Unique reference pubkey (base-58) used as the Solana Pay `reference`
    /// query parameter so the confirming transaction can be found on-chain.
    pub reference: String,
    /// The agent run this payment is associated with.
    pub run_id: String,
    /// Recipient wallet address (base-58).
    pub recipient: String,
    /// Requested amount in SOL (native transfer amount).
    pub amount_sol: f64,
    /// Optional SPL token mint address. `None` means native SOL transfer.
    pub spl_token: Option<String>,
    /// Optional human-readable label shown in the wallet UI.
    pub label: Option<String>,
    /// Optional memo attached to the transaction.
    pub memo: Option<String>,
    /// Current payment status.
    pub status: SolanaPayStatus,
    /// ISO-8601 creation timestamp.
    pub created_at: String,
}

impl SolanaPayIntent {
    /// Return a short text representation of the current status for SQL storage.
    pub fn status_text(&self) -> &str {
        match self.status {
            SolanaPayStatus::Pending => "pending",
            SolanaPayStatus::Confirmed => "confirmed",
            SolanaPayStatus::Failed => "failed",
            SolanaPayStatus::Expired => "expired",
        }
    }

    /// Build the `solana:` Transfer Request URL a wallet scans or opens to
    /// approve this payment — see
    /// <https://docs.solanapay.com/spec#transfer-request>.
    ///
    /// This is the actual "human approves the transaction" artifact: nothing
    /// in this codebase can complete a transfer without the holder of
    /// `recipient`'s counterpart wallet opening this URL and signing it
    /// themselves.
    #[must_use]
    pub fn payment_url(&self) -> String {
        let mut url = format!(
            "solana:{}?amount={}&reference={}",
            self.recipient, self.amount_sol, self.reference
        );
        if let Some(spl_token) = &self.spl_token {
            url.push_str(&format!("&spl-token={}", urlencoding::encode(spl_token)));
        }
        if let Some(label) = &self.label {
            url.push_str(&format!("&label={}", urlencoding::encode(label)));
        }
        if let Some(memo) = &self.memo {
            url.push_str(&format!("&memo={}", urlencoding::encode(memo)));
        }
        url
    }
}

/// Generate a fresh, unique reference key for a new payment intent.
///
/// Solana Pay's `reference` is included in the confirming transaction as a
/// read-only, non-signer account purely so the transaction can later be
/// found via `getSignaturesForAddress` — it does not need to be a valid
/// point on the ed25519 curve (the same way Program Derived Addresses are
/// deliberately off-curve), so 32 CSPRNG bytes are sufficient; no keypair
/// needs to be generated or discarded.
///
/// # Errors
/// Returns an error if the OS CSPRNG is unavailable — this should not
/// normally happen on any supported desktop platform.
pub fn generate_reference() -> Result<String, getrandom::Error> {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes)?;
    Ok(bs58::encode(bytes).into_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_intent() -> SolanaPayIntent {
        SolanaPayIntent {
            reference: "Ref11111111111111111111111111111111111111".to_owned(),
            run_id: "run-1".to_owned(),
            recipient: "Recipient1111111111111111111111111111111111".to_owned(),
            amount_sol: 0.01,
            spl_token: None,
            label: Some("TxODDS Agent Desk".to_owned()),
            memo: Some("run:run-1".to_owned()),
            status: SolanaPayStatus::Pending,
            created_at: "2026-01-01T00:00:00.000Z".to_owned(),
        }
    }

    #[test]
    fn generate_reference_produces_unique_non_empty_values() {
        let a = generate_reference().expect("csprng available in test env");
        let b = generate_reference().expect("csprng available in test env");
        assert!(!a.is_empty());
        assert_ne!(a, b);
    }

    #[test]
    fn payment_url_has_required_fields() {
        let url = sample_intent().payment_url();
        assert!(url.starts_with("solana:Recipient1111111111111111111111111111111111?"));
        assert!(url.contains("amount=0.01"));
        assert!(url.contains("reference=Ref11111111111111111111111111111111111111"));
        assert!(url.contains("label=TxODDS"));
        assert!(url.contains("memo=run%3Arun-1"));
        assert!(!url.contains("spl-token="));
    }

    #[test]
    fn payment_url_includes_spl_token_when_present() {
        let mut intent = sample_intent();
        intent.spl_token = Some("Mint1111111111111111111111111111111111111111".to_owned());
        let url = intent.payment_url();
        assert!(url.contains("spl-token=Mint1111111111111111111111111111111111111111"));
    }
}
