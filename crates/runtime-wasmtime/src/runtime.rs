// adapted from rustforge::crates::runtime-wasmtime on 2026-05-05 — engine_wasmtime feature activated
//! WASM Component-Model loader + capability-gated plugin registry.
//!
//! This module is the **runtime cap gate** that applies to dynamically-
//! loaded `.wasm` plugins (third-party plugins shipped as `.wasm`
//! files where the effects bitmask is unknown until the binary is
//! parsed).
//!
//! The minimal "wasm component" understood by this loader is:
//!
//! ```text
//! [magic 4][version 4][optional rcad-effects section][...module bytes ignored]
//! ```
//!
//! …where the rcad-effects section is encoded as
//! `"rcad-effects:" <comma-separated-effect-tags> ";"` ASCII inside a
//! standard custom section header. Real Component-Model parsing is the
//! responsibility of the `rge-runtime-wasmtime-engine` sibling crate
//! (which uses the wasmtime engine to compile + instantiate); this
//! module's job is only the cap-gating manifest scan.

use crate::effect_specifier::{
    grant_check, BoundPlugin, CapMarker, CapTicket, Capability, EffectSet, GrantError, Plugin,
};
use crate::host::HostState;

/// Minimum sane size of a wasm blob: 4 bytes magic + 4 bytes version.
pub const WASM_HEADER_BYTES: usize = 8;

/// Standard wasm magic header, ASCII "\0asm".
pub const WASM_MAGIC: [u8; 4] = [0x00, 0x61, 0x73, 0x6D];

/// Wasm core version 1, little-endian u32.
pub const WASM_VERSION_V1: [u8; 4] = [0x01, 0x00, 0x00, 0x00];

/// Wasm Component-Model layer version (preview spec).
pub const WASM_VERSION_COMPONENT_PREVIEW: [u8; 4] = [0x0A, 0x00, 0x01, 0x00];

/// Errors raised by the WASM loader.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// Blob shorter than the wasm header.
    #[error("wasm blob too short: {0} bytes (need ≥{WASM_HEADER_BYTES})")]
    TooShort(usize),

    /// First four bytes are not the wasm magic.
    #[error("wasm magic mismatch: expected {expected:?}, got {got:?}")]
    BadMagic {
        /// Expected magic (constant `WASM_MAGIC`).
        expected: [u8; 4],
        /// Actually-observed first 4 bytes.
        got: [u8; 4],
    },

    /// Version bytes don't match v1 core or Component-Model preview.
    #[error("unsupported wasm version: {got:?}")]
    UnsupportedVersion {
        /// Observed version bytes.
        got: [u8; 4],
    },

    /// rcad-effects section was malformed (bad UTF-8, unknown tag, ...).
    #[error("malformed rcad-effects manifest: {0}")]
    BadManifest(String),

    /// Capability ticket did not satisfy the plugin's declared effects.
    #[error("capability gate rejected plugin: {0}")]
    CapabilityGate(#[from] GrantError),
}

/// Outcome of loading + validating + cap-gating a wasm blob.
#[derive(Debug)]
pub struct LoadedPlugin {
    /// blake3 hash of the entire input blob — the audit-log key.
    pub plugin_id: blake3::Hash,
    /// Plugin's declared effect set (from rcad-effects manifest).
    pub effects: EffectSet,
    /// Source-level plugin name.
    pub name: String,
    /// The original bytes — owned so the engine sibling crate can
    /// pass them to wasmtime without re-reading from disk.
    pub bytes: Vec<u8>,
}

impl LoadedPlugin {
    /// Recover the plugin's source-level name.
    pub fn name_static(&self) -> &str {
        &self.name
    }
}

// ---------------------------------------------------------------------------
// Loader
// ---------------------------------------------------------------------------

/// Parse and validate a `.wasm` blob, returning its declared effect
/// set + plugin id.
///
/// # Errors
/// - `LoadError::TooShort` if the blob is shorter than the wasm header.
/// - `LoadError::BadMagic` if the first 4 bytes aren't `\0asm`.
/// - `LoadError::UnsupportedVersion` for unknown wasm version bytes.
/// - `LoadError::BadManifest` if the rcad-effects section is malformed.
pub fn load_wasm_blob(name: impl Into<String>, bytes: &[u8]) -> Result<LoadedPlugin, LoadError> {
    if bytes.len() < WASM_HEADER_BYTES {
        return Err(LoadError::TooShort(bytes.len()));
    }
    let magic: [u8; 4] = bytes[0..4].try_into().expect("4-byte slice");
    if magic != WASM_MAGIC {
        return Err(LoadError::BadMagic {
            expected: WASM_MAGIC,
            got: magic,
        });
    }
    let version: [u8; 4] = bytes[4..8].try_into().expect("4-byte slice");
    if version != WASM_VERSION_V1 && version != WASM_VERSION_COMPONENT_PREVIEW {
        return Err(LoadError::UnsupportedVersion { got: version });
    }

    let tail = &bytes[WASM_HEADER_BYTES..];
    let effects = parse_rcad_effects(tail)?;

    let plugin_id = blake3::hash(bytes);
    Ok(LoadedPlugin {
        plugin_id,
        effects,
        name: name.into(),
        bytes: bytes.to_vec(),
    })
}

/// Parse the rcad-effects manifest from a wasm-tail byte slice. Returns
/// [`EffectSet::EMPTY`] if no manifest is present (a pure plugin).
fn parse_rcad_effects(bytes: &[u8]) -> Result<EffectSet, LoadError> {
    const MARKER: &[u8] = b"rcad-effects:";
    let Some(idx) = find_subsequence(bytes, MARKER) else {
        return Ok(EffectSet::EMPTY);
    };
    let after = &bytes[idx + MARKER.len()..];
    let Some(end) = after.iter().position(|&b| b == b';') else {
        return Err(LoadError::BadManifest(
            "rcad-effects manifest missing terminating semicolon".into(),
        ));
    };
    let body = &after[..end];
    let body_str = core::str::from_utf8(body)
        .map_err(|_| LoadError::BadManifest("rcad-effects body not valid UTF-8".into()))?;
    let mut set = EffectSet::EMPTY;
    for tag in body_str
        .split(',')
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
    {
        let Some(eff) = crate::effect_specifier::Effect::from_tag(tag) else {
            return Err(LoadError::BadManifest(format!(
                "unknown effect tag `{tag}`"
            )));
        };
        set = set.with(eff);
    }
    Ok(set)
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

// ---------------------------------------------------------------------------
// WasmRuntime — capability-gated plugin host
// ---------------------------------------------------------------------------

/// Capability-gated plugin host — Path B runtime gate.
pub struct WasmRuntime {
    /// Per-instance audit-log buffers, keyed by plugin id.
    states: std::collections::HashMap<blake3::Hash, HostState>,
    /// Capabilities granted by the issuing authority.
    granted: crate::effect_specifier::CapSet,
}

impl core::fmt::Debug for WasmRuntime {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("WasmRuntime")
            .field("granted", &self.granted)
            .field("instantiated", &self.states.len())
            .finish()
    }
}

impl WasmRuntime {
    /// Construct a runtime with the given grant.
    #[must_use]
    pub fn new(granted: crate::effect_specifier::CapSet) -> Self {
        Self {
            states: std::collections::HashMap::new(),
            granted,
        }
    }

    /// Cap set this runtime was issued.
    #[must_use]
    pub fn granted(&self) -> crate::effect_specifier::CapSet {
        self.granted
    }

    /// Number of plugins currently instantiated.
    #[must_use]
    pub fn instantiated_count(&self) -> usize {
        self.states.len()
    }

    /// Instantiate a loaded plugin. Returns the plugin's [`HostState`]
    /// which receives audit-log entries during plugin calls.
    ///
    /// # Errors
    /// `LoadError::CapabilityGate` if the runtime's grant doesn't cover
    /// the plugin's declared effect set (Path B runtime gate).
    pub fn instantiate(&mut self, loaded: &LoadedPlugin) -> Result<&mut HostState, LoadError> {
        grant_check(loaded.effects, self.granted)?;
        let id = loaded.plugin_id;
        let state = self
            .states
            .entry(id)
            .or_insert_with(|| HostState::new(self.granted, id));
        Ok(state)
    }

    /// Look up an already-instantiated plugin by id (read-only).
    #[must_use]
    pub fn get(&self, id: &blake3::Hash) -> Option<&HostState> {
        self.states.get(id)
    }

    /// Look up an already-instantiated plugin by id (mutable).
    #[must_use]
    pub fn get_mut(&mut self, id: &blake3::Hash) -> Option<&mut HostState> {
        self.states.get_mut(id)
    }

    /// Drop a plugin instance.
    pub fn drop_instance(&mut self, id: &blake3::Hash) -> Option<HostState> {
        self.states.remove(id)
    }
}

// ---------------------------------------------------------------------------
// Phantom-typed `Plugin<EFFECTS>` ↔ `CapTicket<CAPS>` binding helper
// ---------------------------------------------------------------------------

/// Sub-instantiate a `Plugin<EFFECTS>` against a `CapTicket<CAPS>`.
pub fn bind_plugin<const EFFECTS: u32, const CAPS: u32>(
    plugin: Plugin<EFFECTS>,
    ticket: CapTicket<CAPS>,
) -> Result<BoundPlugin<EFFECTS, CAPS>, GrantError> {
    let effects = EffectSet(EFFECTS);
    let granted = crate::effect_specifier::CapSet(<CapTicket<CAPS> as CapMarker>::CAPS);
    grant_check(effects, granted)?;
    Ok(plugin.bind(ticket))
}

/// Convenience: human-readable cap report — what a host granted.
#[must_use]
pub fn cap_report(granted: crate::effect_specifier::CapSet) -> String {
    let mut covered: Vec<&'static str> = Vec::new();
    for cap in Capability::all() {
        if granted.contains(cap) {
            covered.push(cap.code());
        }
    }
    if covered.is_empty() {
        "<no caps granted>".into()
    } else {
        covered.join(", ")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effect_specifier::{CapSet, Effect};

    fn synth_wasm_blob(effects_manifest: &str) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(64);
        bytes.extend_from_slice(&WASM_MAGIC);
        bytes.extend_from_slice(&WASM_VERSION_V1);
        bytes.extend_from_slice(effects_manifest.as_bytes());
        bytes
    }

    #[test]
    fn load_rejects_too_short_blob() {
        let r = load_wasm_blob("x", b"\0a");
        assert!(matches!(r, Err(LoadError::TooShort(2))));
    }

    #[test]
    fn load_rejects_bad_magic() {
        let blob = b"NOTW\x01\x00\x00\x00rcad-effects:<computes>;";
        let r = load_wasm_blob("x", blob);
        assert!(matches!(r, Err(LoadError::BadMagic { .. })));
    }

    #[test]
    fn load_accepts_v1_core_module() {
        let blob = synth_wasm_blob("rcad-effects:<computes>;");
        let p = load_wasm_blob("pure-plugin", &blob).unwrap();
        assert_eq!(p.effects, EffectSet::from_one(Effect::Computes));
        assert_eq!(p.name, "pure-plugin");
    }

    #[test]
    fn runtime_rejects_phi_plugin_without_phi_read_grant() {
        let blob = synth_wasm_blob("rcad-effects:<computes>,<reads-phi>;");
        let loaded = load_wasm_blob("phi", &blob).unwrap();
        let mut rt = WasmRuntime::new(CapSet::from_one(Capability::ComputeExec));
        let r = rt.instantiate(&loaded);
        assert!(matches!(r, Err(LoadError::CapabilityGate(_))));
    }

    #[test]
    fn runtime_accepts_phi_plugin_with_full_grant() {
        let blob = synth_wasm_blob("rcad-effects:<computes>,<reads-phi>;");
        let loaded = load_wasm_blob("phi", &blob).unwrap();
        let mut rt = WasmRuntime::new(CapSet::all());
        let state = rt.instantiate(&loaded).unwrap();
        assert_eq!(state.audit_count(), 0);
    }

    #[test]
    fn cap_report_lists_granted_caps_in_order() {
        let g = CapSet::from_one(Capability::ComputeExec).with(Capability::PhiRead);
        let r = cap_report(g);
        assert!(r.contains("compute.exec"));
        assert!(r.contains("phi.read"));
    }

    #[test]
    fn cap_report_empty_grant() {
        assert_eq!(cap_report(CapSet::EMPTY), "<no caps granted>");
    }
}
