//! Solana Pay transfer-request rail.
//!
//! Solana Pay is treated as a backend-owned payment/proof intent. React may
//! render the URL or QR code, but Rust creates the reference, memo, amount, and
//! observation path so secrets and settlement authority stay out of JavaScript.

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::config::AppConfig;
use crate::error::AppError;
use crate::triton;
use crate::types::{now_iso, AgentRun, Cluster, SettlementReceipt, SettlementStatus};

const LABEL: &str = "World Cup Agent Desk";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentIntentStatus {
    Pending,
    Observed,
    Confirmed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SolanaPayIntent {
    pub run_id: String,
    pub recipient: String,
    pub amount_sol: f64,
    pub spl_token: Option<String>,
    pub reference: String,
    pub label: String,
    pub message: String,
    pub memo: String,
    pub url: String,
    pub status: PaymentIntentStatus,
    pub created_at: String,
    pub signature: Option<String>,
    pub slot: Option<u64>,
}

impl SolanaPayIntent {
    pub fn status_text(&self) -> &'static str {
        match self.status {
            PaymentIntentStatus::Pending => "pending",
            PaymentIntentStatus::Observed => "observed",
            PaymentIntentStatus::Confirmed => "confirmed",
        }
    }
}

pub fn create_intent(config: &AppConfig, run: &AgentRun) -> Result<SolanaPayIntent, AppError> {
    if !config.solana_cluster.eq_ignore_ascii_case("devnet") {
        return Err(AppError::Config(
            "Solana Pay is devnet-only in this desktop demo".to_string(),
        ));
    }

    let recipient = config
        .solana_pay_recipient
        .clone()
        .ok_or_else(|| AppError::Config("SOLANA_PAY_RECIPIENT missing".to_string()))?;
    validate_pubkeyish("SOLANA_PAY_RECIPIENT", &recipient)?;

    if let Some(token) = config.solana_pay_spl_token.as_deref() {
        validate_pubkeyish("SOLANA_PAY_SPL_TOKEN", token)?;
    }

    let amount_sol = payment_amount(config, run);
    if amount_sol <= 0.0 {
        return Err(AppError::InvalidInput(
            "Solana Pay amount must be positive".to_string(),
        ));
    }
    if amount_sol > config.max_devnet_spend_sol {
        return Err(AppError::InvalidInput(format!(
            "Solana Pay amount {amount_sol} exceeds MAX_DEVNET_SPEND_SOL {}",
            config.max_devnet_spend_sol
        )));
    }

    let reference = new_reference();
    let hash_prefix = run
        .delivery
        .as_ref()
        .map(|delivery| delivery.sha256.chars().take(12).collect::<String>())
        .unwrap_or_else(|| "nohash".to_string());
    let memo = format!("WCAD:{}:{hash_prefix}", run.run_id);
    let message = format!(
        "{} settlement for fixture {}",
        run.track, run.trigger.fixture_id
    );
    let url = transfer_url(
        &recipient,
        amount_sol,
        config.solana_pay_spl_token.as_deref(),
        &reference,
        LABEL,
        &message,
        &memo,
    );

    Ok(SolanaPayIntent {
        run_id: run.run_id.clone(),
        recipient,
        amount_sol,
        spl_token: config.solana_pay_spl_token.clone(),
        reference,
        label: LABEL.to_string(),
        message,
        memo,
        url,
        status: PaymentIntentStatus::Pending,
        created_at: now_iso(),
        signature: None,
        slot: None,
    })
}

pub async fn verify_intent(
    client: &Client,
    config: &AppConfig,
    mut intent: SolanaPayIntent,
) -> Result<SolanaPayIntent, AppError> {
    validate_pubkeyish("Solana Pay reference", &intent.reference)?;
    let sigs = triton::rpc::triton_rpc(
        client,
        config,
        Cluster::Devnet,
        "getSignaturesForAddress",
        json!([intent.reference, { "limit": 5 }]),
    )
    .await?;

    if let Some(item) = first_signature(&sigs) {
        intent.signature = item
            .get("signature")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        intent.slot = item.get("slot").and_then(Value::as_u64);
        intent.status =
            if item.get("confirmationStatus").and_then(Value::as_str) == Some("finalized") {
                PaymentIntentStatus::Confirmed
            } else {
                PaymentIntentStatus::Observed
            };
    }

    Ok(intent)
}

pub fn receipt_from_intent(intent: &SolanaPayIntent) -> SettlementReceipt {
    SettlementReceipt {
        rail: Some("solana_pay".to_string()),
        status: match intent.status {
            PaymentIntentStatus::Pending => SettlementStatus::NotStarted,
            PaymentIntentStatus::Observed => SettlementStatus::Deposited,
            PaymentIntentStatus::Confirmed => SettlementStatus::Deposited,
        },
        reference: Some(intent.reference.clone()),
        escrow_pda: None,
        deposit_tx: intent.signature.clone(),
        release_tx: None,
        explorer_url: intent
            .slot
            .map(|slot| format!("https://explorer.solana.com/block/{slot}?cluster=devnet")),
        triton_observed: Some(matches!(
            intent.status,
            PaymentIntentStatus::Observed | PaymentIntentStatus::Confirmed
        )),
        triton_slot: intent.slot,
        payment_url: Some(intent.url.clone()),
        payment_reference: Some(intent.reference.clone()),
        payment_memo: Some(intent.memo.clone()),
        payment_signature: intent.signature.clone(),
        payment_status: Some(intent.status_text().to_string()),
        payment_recipient: Some(intent.recipient.clone()),
        payment_amount_sol: Some(intent.amount_sol),
    }
}

fn payment_amount(config: &AppConfig, run: &AgentRun) -> f64 {
    let bid_amount = run.winner.as_ref().map(|winner| winner.price_sol);
    let requested = bid_amount.unwrap_or(config.solana_pay_default_amount_sol);
    requested.min(config.max_devnet_spend_sol).max(
        config
            .solana_pay_default_amount_sol
            .min(config.max_devnet_spend_sol),
    )
}

fn transfer_url(
    recipient: &str,
    amount_sol: f64,
    spl_token: Option<&str>,
    reference: &str,
    label: &str,
    message: &str,
    memo: &str,
) -> String {
    let mut params = vec![
        format!("amount={}", decimal_amount(amount_sol)),
        format!("reference={}", urlencoding::encode(reference)),
        format!("label={}", urlencoding::encode(label)),
        format!("message={}", urlencoding::encode(message)),
        format!("memo={}", urlencoding::encode(memo)),
    ];
    if let Some(token) = spl_token {
        params.insert(1, format!("spl-token={}", urlencoding::encode(token)));
    }
    format!("solana:{recipient}?{}", params.join("&"))
}

fn decimal_amount(amount: f64) -> String {
    let mut value = format!("{amount:.9}");
    while value.contains('.') && value.ends_with('0') {
        value.pop();
    }
    if value.ends_with('.') {
        value.pop();
    }
    value
}

fn new_reference() -> String {
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let mut bytes = [0_u8; 32];
    bytes[..16].copy_from_slice(first.as_bytes());
    bytes[16..].copy_from_slice(second.as_bytes());
    bs58::encode(bytes).into_string()
}

fn first_signature(value: &Value) -> Option<&Value> {
    value.as_array().and_then(|items| items.first())
}

fn validate_pubkeyish(name: &str, value: &str) -> Result<(), AppError> {
    let bytes = bs58::decode(value)
        .into_vec()
        .map_err(|_| AppError::InvalidInput(format!("{name} must be base58")))?;
    if bytes.len() != 32 {
        return Err(AppError::InvalidInput(format!(
            "{name} must decode to 32 bytes"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_reference_is_base58_32_bytes() {
        let reference = new_reference();
        let bytes = bs58::decode(reference).into_vec().unwrap();
        assert_eq!(bytes.len(), 32);
    }

    #[test]
    fn transfer_url_percent_encodes_text_fields() {
        let url = transfer_url(
            "11111111111111111111111111111111",
            0.01,
            None,
            "11111111111111111111111111111111",
            "World Cup Agent Desk",
            "fixture 17 settlement",
            "WCAD:run id:abcdef",
        );
        assert!(url.contains("label=World%20Cup%20Agent%20Desk"));
        assert!(url.contains("message=fixture%2017%20settlement"));
        assert!(url.contains("memo=WCAD%3Arun%20id%3Aabcdef"));
    }
}
