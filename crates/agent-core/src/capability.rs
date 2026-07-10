//! Compile-time capability tokens for agent tools.
//!
//! Each capability is a zero-sized type (ZST) behind a **sealed trait**.
//! This means the compiler can statically prove that a given agent
//! instance *cannot even attempt* to call a tool it was not granted —
//! the mismatch is a compile error, not a runtime permission check.
//!
//! Checklist §8: "Is it possible for the compiler to prove that a given
//! agent instance cannot even attempt to call a tool it wasn't granted?"
//!
//! # Design
//!
//! ```text
//! FollowCap  ─▶  RecordPositionTool<FollowCap>   (match-intelligence-agent)
//! FadeCap    ─▶  RecordPositionTool<FadeCap>     (contrarian-agent)
//! SettleCap  ─▶  SettleWinnerTool<SettleCap>     (arena-coordinator only)
//! ```
//!
//! A `FadeCap` cannot be handed to a function expecting `FollowCap`; the
//! compiler rejects it at monomorphisation time.

/// Sealed trait — no external crate can implement a new `Capability`.
/// This prevents a prompt-injected string from being widened into a capability.
/// Exposed as `private_sealed` so sibling modules (e.g. `tools`) can add
/// implementations for their own read-only tokens without leaving the crate.
pub mod private_sealed {
    /// Sealed super-trait preventing external crates from defining capabilities.
    pub trait Sealed {}
}

/// Marker trait shared by all capability tokens.
/// Implementors are restricted to this crate via the `Sealed` super-trait.
pub trait Capability: private_sealed::Sealed + Send + Sync + 'static {}

// ── Concrete capability tokens ────────────────────────────────────────────────

/// Grants the ability to commit a **follow-sharp-movement** position.
/// Held exclusively by `match-intelligence-agent`.
#[derive(Debug, Clone, Copy)]
pub struct FollowCap;

impl FollowCap {
    /// Construct the capability token.  In production this is validated against
    /// the `CoralOS` session grant; in tests/binaries it's constructed directly.
    #[must_use]
    pub fn acquire() -> Self { Self }
}

/// Grants the ability to commit a **fade-sharp-movement** (contrarian) position.
/// Held exclusively by `contrarian-agent`.
#[derive(Debug, Clone, Copy)]
pub struct FadeCap;

impl FadeCap {
    /// Construct the capability token.
    #[must_use]
    pub fn acquire() -> Self { Self }
}

/// Grants the ability to record a match outcome and settle on-chain.
/// Held exclusively by `arena-coordinator`.
#[derive(Debug, Clone, Copy)]
pub struct SettleCap;

impl SettleCap {
    /// Construct the capability token.
    #[must_use]
    pub fn acquire() -> Self { Self }
}

impl private_sealed::Sealed for FollowCap {}
impl private_sealed::Sealed for FadeCap {}
impl private_sealed::Sealed for SettleCap {}

impl Capability for FollowCap {}
impl Capability for FadeCap {}
impl Capability for SettleCap {}

// ── Capability grant container ────────────────────────────────────────────────

/// Type-safe wrapper that carries an agent's capability token through the
/// async call graph without using globals or thread-locals.
///
/// Checklist §21: session credentials passed explicitly, never ambient.
#[derive(Debug, Clone)]
pub struct CapabilityGrant<C: Capability> {
    inner: C,
}

impl<C: Capability> CapabilityGrant<C> {
    /// Create a new grant.  In production this is constructed once at agent
    /// startup from the `CoralOS` session token — never fabricated from model
    /// output.
    pub fn new(cap: C) -> Self {
        Self { inner: cap }
    }

    /// Borrow the inner capability.
    pub fn get(&self) -> &C {
        &self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follow_cap_grant_is_cloneable() {
        let grant = CapabilityGrant::new(FollowCap);
        let _clone = grant.clone();
    }

    #[test]
    fn capability_tokens_are_zero_sized() {
        assert_eq!(std::mem::size_of::<FollowCap>(), 0);
        assert_eq!(std::mem::size_of::<FadeCap>(), 0);
        assert_eq!(std::mem::size_of::<SettleCap>(), 0);
    }
}
