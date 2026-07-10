//! Solana Pay commands — the wallet-approval-before-settlement flow.
//!
//! rig-venice ROADMAP.md Phase 7, item 3. Before this module, `create_solana_
//! pay_intent` / `verify_solana_pay_intent` / `list_payment_intents` existed
//! as frontend calls (`ui/desktop/transport.ts`) into Tauri commands that did
//! not exist at all — not merely unwired, genuinely missing. Calling them
//! would have failed at runtime with "command not found".
//!
//! The flow this enables: the UI creates an intent for a wager's stake,
//! shows the user the resulting `solana:` payment URL, the user opens it in
//! their own wallet and approves the transfer, and `verify_solana_pay_intent`
//! (polled or invoked manually) checks the chain for a transaction
//! referencing this intent's `reference` key. Nothing here can move funds by
//! itself — it only requests a transfer a human's wallet has to sign.

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::error::AppError;
use crate::event_bus;
use crate::services::chain;
use crate::services::solana_pay::{generate_reference, SolanaPayIntent, SolanaPayStatus};
use crate::state::DesktopState;
use crate::types::{now_iso, Cluster};

/// `SolanaPayIntent` plus its computed payment URL, for the frontend to
/// render directly — the URL isn't persisted (it's derived from the intent's
/// own fields), so callers get it alongside the intent rather than having to
/// duplicate `SolanaPayIntent::payment_url()`'s logic in TypeScript.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentIntentView {
    #[serde(flatten)]
    pub intent: SolanaPayIntent,
    pub payment_url: String,
}

impl From<SolanaPayIntent> for PaymentIntentView {
    fn from(intent: SolanaPayIntent) -> Self {
        let payment_url = intent.payment_url();
        Self { intent, payment_url }
    }
}

/// Create a new Solana Pay payment intent for a run.
///
/// `amount_sol` and `label`/`memo` default from config/run when omitted so
/// the UI can pass the wager's actual stake when one is available, or fall
/// back to the configured default amount otherwise.
#[tauri::command]
pub async fn create_solana_pay_intent(
    run_id: String,
    amount_sol: Option<f64>,
    label: Option<String>,
    memo: Option<String>,
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<PaymentIntentView, AppError> {
    let recipient = state
        .config
        .solana_pay_recipient
        .clone()
        .ok_or_else(|| AppError::Config("SOLANA_PAY_RECIPIENT is not configured".to_string()))?;

    let reference = generate_reference()
        .map_err(|e| AppError::Task(format!("failed to generate payment reference: {e}")))?;

    let intent = SolanaPayIntent {
        reference,
        run_id: run_id.clone(),
        recipient,
        amount_sol: amount_sol.unwrap_or(state.config.solana_pay_default_amount_sol),
        spl_token: state.config.solana_pay_spl_token.clone(),
        label: label.or_else(|| Some(format!("TxODDS Agent Desk — run {run_id}"))),
        memo: memo.or_else(|| Some(format!("run:{run_id}"))),
        status: SolanaPayStatus::Pending,
        created_at: now_iso(),
    };

    {
        let ledger = state.ledger.lock().map_err(|_| AppError::LockPoisoned)?;
        ledger.upsert_payment_intent(&intent)?;
    }

    // Register a live watch so a confirming transaction is observed as soon
    // as it lands, in addition to whatever manual/polled verify calls happen.
    if let Some(yellowstone) = &state.yellowstone {
        yellowstone.watch_reference(intent.reference.clone());
    }

    let _ = app.emit(event_bus::PAY_INTENT, &intent);
    Ok(intent.into())
}

/// Check whether a payment intent's reference has landed on-chain, and
/// update its status accordingly.
///
/// Idempotent: intents already `Confirmed`/`Failed`/`Expired` are returned
/// unchanged without a redundant RPC call.
#[tauri::command]
pub async fn verify_solana_pay_intent(
    reference: String,
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<PaymentIntentView, AppError> {
    let mut intent = {
        let ledger = state.ledger.lock().map_err(|_| AppError::LockPoisoned)?;
        ledger.get_payment_intent_by_reference(&reference)?
    };

    if intent.status != SolanaPayStatus::Pending {
        return Ok(intent.into());
    }

    // A landed transaction includes the reference as a read-only account, so
    // it shows up in getSignaturesForAddress for that "address" even though
    // it never received any funds directly itself.
    let signatures = chain::rpc::solana_rpc(
        &state.client,
        &state.config,
        Cluster::Devnet,
        "getSignaturesForAddress",
        serde_json::json!([&intent.reference, { "limit": 1 }]),
    )
    .await?;

    let confirmed = signatures
        .as_array()
        .is_some_and(|entries| !entries.is_empty());

    if confirmed {
        intent.status = SolanaPayStatus::Confirmed;
        let ledger = state.ledger.lock().map_err(|_| AppError::LockPoisoned)?;
        ledger.upsert_payment_intent(&intent)?;
        let _ = app.emit(event_bus::PAY_STATUS, &intent);
    }

    Ok(intent.into())
}

/// List payment intents, optionally filtered to one run.
#[tauri::command]
pub async fn list_payment_intents(
    run_id: Option<String>,
    state: State<'_, DesktopState>,
) -> Result<Vec<PaymentIntentView>, AppError> {
    let ledger = state.ledger.lock().map_err(|_| AppError::LockPoisoned)?;
    let intents = ledger.list_payment_intents(run_id.as_deref())?;
    Ok(intents.into_iter().map(PaymentIntentView::from).collect())
}
