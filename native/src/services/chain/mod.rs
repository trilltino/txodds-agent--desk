//! Chain service: Triton One integration.
//!
//! Allowlisted JSON-RPC over the Triton RPC pool plus Yellowstone gRPC
//! (Dragon's Mouth) stream supervision. All x-tokens stay in Rust.

pub mod rpc;
pub mod yellowstone;
