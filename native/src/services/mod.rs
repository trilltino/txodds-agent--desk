//! Backend services: async side-effect units behind the Tauri command layer.
//!
//! Naming follows the lean-track vocabulary (docs/architecture/01-lean-e2e-architecture.md):
//! a *service* owns I/O and supervision; deterministic business logic belongs in
//! engines/domain modules; only the future Match Intelligence runtime is an *agent*.
//!
//! - `txline`: TxLINE HTTP data client plus live ingest supervision.
//! - `chain`: Triton One integration - allowlisted JSON-RPC and the Yellowstone
//!   gRPC sidecar supervisor.
//! - `ledger`: SQLite persistence for runs, receipts, and payment intents.
//! - `coral`: legacy Coral-style round engine and CoralOS settlement bridge,
//!   kept as the compatibility path until the Match Intelligence Agent lands
//!   (see docs/adr/0006-lean-agent-runtime-no-agent-theatre.md).
//! - `coralos`: first-class Coral session/transcript protocol around the
//!   compatibility engine and future external Coral transport.
//! - `agent`: Match Intelligence Agent trace/tool orchestration.
//! - `llm`: optional Venice/OpenAI-compatible explanation client.
//! - `proof`: proof receipt and validation simulation state.
//! - `user_store`: sled-backed local profile KV store (public key → UserProfile).
//! - `backtest`: arena replay engine — walks real historical TxLINE odds and
//!   settles simulated FollowSharp/FadeSharp positions against the real final
//!   score (ARENA-AUTONOMY-PLAN.md Priority B).
//! - `autonomous`: live-trigger poll loop that calls `run_match_intelligence_round`
//!   without a human clicking Analyze (ARENA-AUTONOMY-PLAN.md Priority A).

pub mod agent;
pub mod autonomous;
pub mod backtest;
pub mod chain;
pub mod coral;
pub mod coralos;
pub mod ledger;
pub mod llm;
pub mod proof;
pub mod solana_pay;
pub mod txline;
pub mod user_store;
