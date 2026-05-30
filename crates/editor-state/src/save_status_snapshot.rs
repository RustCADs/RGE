//! Read-only save-state observation snapshot for the editor's bottom
//! status bar — the open scene's file name + the Command-Bus dirty flag.
//!
//! # Sibling to [`crate::InspectorSnapshot`], not a 6th coordination category
//!
//! Like [`crate::InspectorSnapshot`], this is a read-only **observation
//! aggregator** — a plain, owned view of editor-session save state assembled
//! by `editor-shell` from already-public accessors (`scene_source_path`,
//! `command_bus().is_dirty()`) for consumption by the `editor-ui` status-bar
//! widget. It owns no state, stores no IDs; `editor-shell` produces a fresh
//! instance per frame via `EditorShell::save_status_snapshot()`. The §0.6
//! freeze gates the *coordination-category* count at 5 (Selection, Hover,
//! ActiveTool, ModalState, DragDrop); this is not one of them, so the
//! `editor-state-ownership` lint Part A (which forbids only those five names
//! outside editor-state) does not fire.
//!
//! Living here (rather than in `editor-shell` directly) keeps the
//! editor-shell ↔ editor-ui hosting direction open: both crates already
//! depend on `editor-state`, so a shared observation type avoids forcing
//! either crate to depend on the other — the same rationale that places
//! [`crate::InspectorSnapshot`] here.
//!
//! # Why `Clone`, not `Copy`
//!
//! [`crate::InspectorSnapshot`] is a `Copy` flat bag of leaves. This type
//! carries an owned `String` (`scene_file_name`), so it is `Clone` but **not**
//! `Copy`. That is intentional and is precisely why the scene name does NOT
//! live on `InspectorSnapshot` (which must stay `Copy` per its documented
//! invariant): a non-`Copy` `String` field there would break that invariant.
//! This snapshot crosses the editor-shell → host handoff as an
//! `Arc<SaveStatusSnapshot>` (cheap to share), mirroring the inspector handoff.
//!
//! # Architectural invariants (shared with `InspectorSnapshot`)
//!
//! - **Single source per field.** `scene_file_name` is derived once from
//!   `EditorShell::scene_source_path()`; `is_dirty` mirrors
//!   `CommandBus::is_dirty()`. No staleness, no caching.
//! - **No side effects on construction.** Building the snapshot is a pure
//!   read; no audit-ledger events, no bus submits.

/// Plain-data view of editor save state for the headless status-bar model.
/// Built by `EditorShell::save_status_snapshot()` as a pure read with no side
/// effects; rendered by `rge_editor_ui::widgets::save_status`.
///
/// # Field stability
///
/// - `scene_file_name`: the file name (no directory) of the open `.rge-scene`
///   silent-save source, pre-extracted via `Path::file_name` in the producer
///   so the formatter does no path I/O. `Some(name)` after opening / launching
///   a `.rge-scene` or a successful Save-As; `None` for a blank / demo /
///   `.glb` / `.rge-project` context (mirrors `EditorShell::scene_source_path`
///   presence).
/// - `is_dirty`: mirror of `CommandBus::is_dirty`; `true` when there are
///   unsaved edits (the bus cursor is past the last `mark_saved`).
///
/// # Trait bounds
///
/// `Clone + Debug + PartialEq + Default` — consumers can store, diff, and
/// `Debug`-format snapshots. `Send + Sync` are auto-derived (so an
/// `Arc<SaveStatusSnapshot>` can cross the handoff). Intentionally **not**
/// `Copy` (carries an owned `String`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SaveStatusSnapshot {
    /// File name of the open `.rge-scene` source, if any (no directory).
    pub scene_file_name: Option<String>,
    /// `CommandBus::is_dirty()` — `true` when there are unsaved edits.
    pub is_dirty: bool,
}
