//! TxLINE ingestion module.

pub mod api;
mod ingest;

pub use ingest::spawn_txline;
