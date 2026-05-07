// adapted from rustforge::crates::runtime-wasmtime on 2026-05-05 — engine_wasmtime feature activated
//! Capability-ticket helpers — convenience re-exports + a v0 ticket
//! issuer that wraps the const-generic [`CapTicket`] in a runtime
//! value-typed surface useful for tests and dynamic plugin loaders.
//!
//! The const-generic [`CapTicket<CAPS>`] in [`crate::effect_specifier`]
//! is the sealed authority. This module ships the lightweight runtime
//! mirror for callers that don't know the cap mask at compile time
//! (for example: dynamic `.wasm` loaders that read a manifest file
//! and decide CAPS at runtime).

use crate::effect_specifier::{CapSet, GrantError};

/// Runtime-typed "ticket" carrying a `CapSet` value rather than a
/// const-generic mask. Used by [`crate::runtime::WasmRuntime`] when
/// the caller doesn't know caps until manifest parse-time.
#[derive(Debug, Clone, Copy)]
pub struct DynCapTicket {
    /// Granted capability set (runtime value).
    pub caps: CapSet,
    /// Plugin-id this ticket was issued to.
    pub plugin_id: blake3::Hash,
    /// Issuing authority.
    pub issuer: &'static str,
}

impl DynCapTicket {
    /// Construct a runtime ticket.
    #[must_use]
    pub fn new(caps: CapSet, plugin_id: blake3::Hash, issuer: &'static str) -> Self {
        Self {
            caps,
            plugin_id,
            issuer,
        }
    }

    /// Verify this ticket's caps cover the given effect set.
    ///
    /// # Errors
    ///
    /// - [`GrantError`] when the requested `effects` include any effect
    ///   not covered by the ticket's [`CapSet`].
    pub fn check_covers(
        &self,
        effects: crate::effect_specifier::EffectSet,
    ) -> Result<(), GrantError> {
        crate::effect_specifier::grant_check(effects, self.caps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effect_specifier::{Capability, Effect, EffectSet};

    #[test]
    fn dyn_cap_ticket_covers_phi_when_phi_read_granted() {
        let t = DynCapTicket::new(
            CapSet::from_one(Capability::ComputeExec).with(Capability::PhiRead),
            blake3::hash(b"x"),
            "test",
        );
        assert!(t
            .check_covers(EffectSet::from_one(Effect::ReadsPhi))
            .is_ok());
    }

    #[test]
    fn dyn_cap_ticket_rejects_phi_without_phi_read() {
        let t = DynCapTicket::new(
            CapSet::from_one(Capability::ComputeExec),
            blake3::hash(b"x"),
            "test",
        );
        assert!(t
            .check_covers(EffectSet::from_one(Effect::ReadsPhi))
            .is_err());
    }
}
