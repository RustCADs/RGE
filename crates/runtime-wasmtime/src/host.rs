// adapted from rustforge::crates::runtime-wasmtime on 2026-05-05 — engine_wasmtime feature activated
//! Host execution context for WIT-typed WASM Component-Model plugins.
//!
//! `HostState` carries the per-instance data a plugin call may touch:
//! the active capability ticket, an audit-log buffer, and a small
//! tick counter the W04 hello-world test increments.

use crate::effect_specifier::{CapSet, EffectSet, GrantError};

/// Per-instance host context handed to a plugin call.
///
/// The fields are intentionally small. The `audit_log` buffer is a
/// `Vec<String>` so tests can `format!`-inspect what mutations a
/// `<transacts>`-marked plugin would have made. The `tick_counter`
/// field is the W04 hello-world counter — incremented from the wasm
/// `tick(dt)` export each frame.
#[derive(Debug)]
pub struct HostState {
    /// Capabilities the issuer granted.
    pub caps: CapSet,
    /// In-memory audit-log buffer; one line per host-side mutation.
    pub audit_log: Vec<String>,
    /// Plugin-id (blake3 of the .wasm bytes); echoed into every audit
    /// line for traceability.
    pub plugin_id: blake3::Hash,
    /// W04 hello-world tick counter — bumped from the wasm `tick(dt)`
    /// export and read back from the host integration test.
    pub tick_counter: u32,
    /// W04 last-tick-dt — the f32 the wasm module passed to its
    /// host-bound `host_record_tick` import. Useful for asserting the
    /// host actually saw the dt the wasm module was given.
    pub last_dt: f32,
}

impl HostState {
    /// Construct a host state for a freshly issued ticket.
    #[must_use]
    pub fn new(caps: CapSet, plugin_id: blake3::Hash) -> Self {
        Self {
            caps,
            audit_log: Vec::new(),
            plugin_id,
            tick_counter: 0,
            last_dt: 0.0,
        }
    }

    /// Verify that this host has the capabilities required to honour
    /// the plugin's declared effect set.
    ///
    /// # Errors
    /// Returns `GrantError::MissingCapability` if the host's `caps`
    /// don't cover every capability required by `effects`.
    pub fn check_grants(&self, effects: EffectSet) -> Result<(), GrantError> {
        crate::effect_specifier::grant_check(effects, self.caps)
    }

    /// Append an audit-log entry. `effect` records *which* declared
    /// effect the call exercised so the auditor can correlate
    /// mutation pattern with the plugin's declared envelope.
    pub fn audit(&mut self, effect: crate::effect_specifier::Effect, message: impl AsRef<str>) {
        let line = format!(
            "[plugin={short_id}] {effect_tag} {message}",
            short_id = short_hash(&self.plugin_id),
            effect_tag = effect.as_tag(),
            message = message.as_ref(),
        );
        self.audit_log.push(line);
    }

    /// Number of audit-log lines emitted so far.
    #[must_use]
    pub fn audit_count(&self) -> usize {
        self.audit_log.len()
    }

    /// Replay the audit log as a single newline-joined string.
    #[must_use]
    pub fn audit_replay(&self) -> String {
        self.audit_log.join("\n")
    }

    /// Increment the W04 hello-world tick counter and stash `dt`.
    /// Called from the wasm-bound `host_record_tick` host function.
    pub fn record_tick(&mut self, dt: f32) {
        self.tick_counter = self.tick_counter.saturating_add(1);
        self.last_dt = dt;
    }
}

/// 8-character truncation of a blake3 hash, in lowercase hex —
/// short enough to be readable in log lines but unambiguous.
#[must_use]
pub fn short_hash(h: &blake3::Hash) -> String {
    let bytes = h.as_bytes();
    let mut out = String::with_capacity(8);
    for &b in &bytes[..4] {
        use core::fmt::Write as _;
        let _ = write!(out, "{b:02x}");
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effect_specifier::{Capability, Effect};

    #[test]
    fn host_state_starts_empty() {
        let h = HostState::new(CapSet::EMPTY, blake3::hash(b"plugin-a"));
        assert_eq!(h.audit_count(), 0);
        assert_eq!(h.audit_replay(), "");
        assert_eq!(h.tick_counter, 0);
    }

    #[test]
    fn audit_records_plugin_id_short_form() {
        let id = blake3::hash(b"plugin-b");
        let mut h = HostState::new(CapSet::EMPTY, id);
        h.audit(Effect::Computes, "transformed mesh");
        assert_eq!(h.audit_count(), 1);
        let line = &h.audit_log[0];
        assert!(line.contains(&short_hash(&id)));
        assert!(line.contains("<computes>"));
        assert!(line.contains("transformed mesh"));
    }

    #[test]
    fn audit_short_hash_is_8_hex_chars() {
        let h = blake3::hash(b"abc");
        let s = short_hash(&h);
        assert_eq!(s.len(), 8);
        for c in s.chars() {
            let lower_hex = c.is_ascii_hexdigit() && (c.is_ascii_digit() || c.is_ascii_lowercase());
            assert!(lower_hex, "non-lower-hex char in short_hash: {c}");
        }
    }

    #[test]
    fn check_grants_on_phi_read_with_compute_only_rejects() {
        let h = HostState::new(
            CapSet::from_one(Capability::ComputeExec),
            blake3::hash(b"plugin-c"),
        );
        let r = h.check_grants(EffectSet::from_one(Effect::ReadsPhi));
        assert!(matches!(r, Err(GrantError::MissingCapability { .. })));
    }

    #[test]
    fn check_grants_on_phi_read_with_full_grant_accepts() {
        let h = HostState::new(CapSet::all(), blake3::hash(b"plugin-d"));
        let r = h.check_grants(EffectSet::from_one(Effect::ReadsPhi));
        assert!(r.is_ok());
    }

    #[test]
    #[allow(
        clippy::float_cmp,
        reason = "round-tripping the same f32 literal through a setter without arithmetic must yield bit-equal storage; bit-equality is the intended assertion"
    )]
    fn record_tick_increments_counter_and_stashes_dt() {
        let mut h = HostState::new(CapSet::all(), blake3::hash(b"plugin-tick"));
        h.record_tick(0.016);
        assert_eq!(h.tick_counter, 1);
        assert_eq!(h.last_dt, 0.016);
        h.record_tick(0.033);
        assert_eq!(h.tick_counter, 2);
        assert_eq!(h.last_dt, 0.033);
    }
}
