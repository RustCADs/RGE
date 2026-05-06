//! Event types for the audit ledger.
//!
//! The core type is [`Event`], an immutable record appended to
//! [`crate::AuditLedger`].  Each event carries an [`EventId`] — a 32-byte
//! BLAKE3 digest computed deterministically from `(kind_tag, payload)` — and
//! an [`EventKind`] tag that declares ownership of the payload format.

use serde::{Deserialize, Serialize};

// ────────────────────────────────────────────────────────────────────────────
// EventId
// ────────────────────────────────────────────────────────────────────────────

/// 32-byte deterministic event identifier — BLAKE3 hash of `(kind_tag,
/// payload)`.
///
/// Stable across machines for identical input.  Implements `Hash + Eq + Ord`
/// for use in indices and sorted iteration.
///
/// Two events with the same `kind` and `payload` produce the same `EventId`
/// even at different sequence numbers — that is intentional and allows
/// content-addressed de-duplication by callers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EventId(pub [u8; 32]);

impl EventId {
    /// Compute the deterministic ID for a `kind` + `payload` pair.
    ///
    /// The kind tag is mixed in first (as its `kind_tag()` byte slice) so that
    /// different `EventKind` variants with identical payloads produce different
    /// IDs.
    #[must_use]
    pub fn compute(kind: &EventKind, payload: &[u8]) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(kind.kind_tag().as_bytes());
        // Separator byte so "Action" + b"foo" != "Actio" + b"nfoo".
        hasher.update(b"\x00");
        hasher.update(payload);
        Self(*hasher.finalize().as_bytes())
    }

    /// Hex-encode the full 32 bytes for human display.
    ///
    /// Takes `self` by value because [`EventId`] is `Copy`.
    #[must_use]
    pub fn to_hex(self) -> String {
        self.0.iter().fold(String::with_capacity(64), |mut s, b| {
            use std::fmt::Write as _;
            let _ = write!(s, "{b:02x}");
            s
        })
    }
}

impl std::fmt::Display for EventId {
    /// Displays a short `"blake3:<8-hex-char>…"` prefix suitable for logs.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let hex = (*self).to_hex();
        // Show the first 8 hex chars (4 bytes) followed by an ellipsis.
        write!(f, "blake3:{}…", &hex[..8])
    }
}

// ────────────────────────────────────────────────────────────────────────────
// EventKind
// ────────────────────────────────────────────────────────────────────────────

/// Event kind tag.
///
/// Open (`Custom(String)`) so plugins can register their own kinds without
/// forking the ledger crate.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventKind {
    /// `editor-actions::Action` event (Phase 2.2).  Payload format owned by
    /// the Command Bus.
    Action,
    /// CAD checkpoint event (Phase 4-Geometry).  Payload format owned by
    /// `cad-core`.
    CadCheckpoint,
    /// Plugin-defined event.  Payload format owned by the registering plugin.
    Custom(String),
}

impl EventKind {
    /// Return a stable string tag used as the BLAKE3 kind prefix.
    ///
    /// The tag must be unique per variant; changing a tag is a
    /// **breaking change** that invalidates all existing event IDs.
    #[must_use]
    pub fn kind_tag(&self) -> &str {
        match self {
            Self::Action => "Action",
            Self::CadCheckpoint => "CadCheckpoint",
            Self::Custom(name) => name.as_str(),
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Event
// ────────────────────────────────────────────────────────────────────────────

/// One recorded event.
///
/// Append-only — never mutated after [`crate::AuditLedger::record`] returns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    /// Deterministic ID, recomputable from `kind` + `payload`.
    pub id: EventId,
    /// Append index — monotonic per ledger; not deterministic across runs.
    pub seq: u64,
    /// Wall-clock timestamp (millis since UNIX epoch).  For human inspection
    /// only; determinism uses `id` + `seq`, not time.
    pub timestamp_ms: u64,
    /// Event kind.
    pub kind: EventKind,
    /// Opaque serialized payload — format owned by the event producer.  The
    /// ledger does not parse it; downstream replay handlers do.
    pub payload: Vec<u8>,
}

// ────────────────────────────────────────────────────────────────────────────
// Unit tests
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_id_is_deterministic_for_identical_input() {
        let a = EventId::compute(&EventKind::Action, b"hello");
        let b = EventId::compute(&EventKind::Action, b"hello");
        assert_eq!(a, b);
    }

    #[test]
    fn event_id_differs_for_different_kind_same_payload() {
        let action = EventId::compute(&EventKind::Action, b"payload");
        let checkpoint = EventId::compute(&EventKind::CadCheckpoint, b"payload");
        assert_ne!(action, checkpoint);
    }

    #[test]
    fn event_id_differs_for_different_payload_same_kind() {
        let a = EventId::compute(&EventKind::Action, b"foo");
        let b = EventId::compute(&EventKind::Action, b"bar");
        assert_ne!(a, b);
    }

    #[test]
    fn event_id_custom_kind_uses_name_as_tag() {
        let custom = EventId::compute(&EventKind::Custom("MyPlugin".to_owned()), b"data");
        let action = EventId::compute(&EventKind::Action, b"data");
        assert_ne!(custom, action);
    }

    #[test]
    fn event_id_to_hex_is_64_chars() {
        let id = EventId::compute(&EventKind::Action, b"test");
        assert_eq!(id.to_hex().len(), 64);
    }

    #[test]
    fn event_id_display_has_blake3_prefix() {
        let id = EventId::compute(&EventKind::Action, b"test");
        let s = id.to_string();
        assert!(s.starts_with("blake3:"));
        // 7 prefix + 8 hex + 1 ellipsis = 16 chars minimum
        assert!(s.len() >= 16);
    }

    #[test]
    fn kind_tag_separator_prevents_prefix_collision() {
        // "Action" tag + "\x00" + b"foo" must differ from a custom kind
        // whose name is "Action\x00fo" and payload is b"o".
        let a = EventId::compute(&EventKind::Action, b"foo");
        let b = EventId::compute(&EventKind::Custom("Action\x00fo".to_owned()), b"o");
        assert_ne!(a, b);
    }
}
