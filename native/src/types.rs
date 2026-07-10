//! Shared data contracts — all types now live in the standalone `txodds-types`
//! crate so they can be used by `agent-core` and future crates without pulling
//! in Tauri. This module is a thin re-export so every existing
//! `use crate::types::X` in the app continues to compile unchanged.

pub use txodds_types::*;
