//! TxLINE service: documented data API client and ingest supervision.

pub mod api;
mod ingest;

pub use ingest::spawn_txline;
