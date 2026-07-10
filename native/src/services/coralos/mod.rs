//! First-class CoralOS session and transcript primitives.
//!
//! The legacy `services::coral` module still produces the compatibility round.
//! This module is the orchestration fabric around it: typed messages, sessions,
//! replayable transcript artifacts, and external CoralOS Console publishing.

pub mod console;
pub mod protocol;
pub mod transcript;
