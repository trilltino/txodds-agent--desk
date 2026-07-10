//! Coral compatibility utilities and the CoralOS settlement bridge.
//!
//! The active intelligence path lives in `services::agent::runtime`. New
//! product code should grow in the agent modules, while this namespace keeps
//! older settlement helpers available.

pub mod agents;
pub mod settlement;
