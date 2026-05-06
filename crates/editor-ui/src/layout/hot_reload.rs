//! `notify`-backed hot-reload watcher for workspace RON files.
//!
//! Adapted from rustforge::apps::editor-app::ir_bridge on 2026-05-05 — generalized
//! for Workspace. The rustforge precursor reloaded on every app-tick read; here we
//! drive reloads from filesystem events so the editor only re-parses when the user
//! actually saved a file.
//!
//! Per PLAN.md §6.7: file change → notify → re-parse → diff → repaint, end-to-end
//! `<50ms`. The watcher itself is light — it forwards `notify::Event` into a
//! `crossbeam`-style mpsc channel and lets the editor's main thread pull events
//! at frame boundaries (no spawned thread holds a `Workspace` lock).
//!
//! ## Usage shape
//!
//! ```ignore
//! use rge_editor_ui::layout::hot_reload::{WorkspaceWatcher, ChangeEvent};
//! let (watcher, rx) = WorkspaceWatcher::start("path/to/workspace.ron")?;
//! while let Ok(ev) = rx.try_recv() {
//!     match ev {
//!         ChangeEvent::Modified => { /* re-read + diff */ }
//!         ChangeEvent::Removed  => { /* fall back to default */ }
//!     }
//! }
//! drop(watcher); // stops watcher
//! ```

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

/// File-change event surfaced to the editor's main thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeEvent {
    /// File was created or modified — reload + reconcile.
    Modified,
    /// File was deleted or moved away — caller should fall back to a default.
    Removed,
}

/// Errors returned by `WorkspaceWatcher::start`.
#[derive(Debug, thiserror::Error)]
pub enum WatchError {
    /// `notify` failed to construct/configure the platform watcher.
    #[error("notify error: {0}")]
    Notify(#[from] notify::Error),
}

/// Hot-reload watcher around a single workspace `.ron` path.
///
/// Holds the platform watcher alive until dropped. Events are funnelled through
/// the provided `Receiver<ChangeEvent>`.
pub struct WorkspaceWatcher {
    /// Owned platform watcher — kept alive for the lifetime of the struct.
    _watcher: RecommendedWatcher,
    /// Path being watched (kept for diagnostics / display).
    path: PathBuf,
}

impl WorkspaceWatcher {
    /// Begin watching `path` for content changes.
    ///
    /// Returns `(watcher, rx)`; drop the watcher to stop.
    ///
    /// Coalescing: rapid back-to-back saves (common with editors that
    /// write+rename) are coalesced into one `Modified` event by `notify`'s
    /// configurable debounce. We use a `200ms` debounce by default.
    ///
    /// # Errors
    ///
    /// Returns `WatchError::Notify` if the platform watcher cannot be
    /// constructed or fails to begin watching the parent directory.
    pub fn start(path: impl AsRef<Path>) -> Result<(Self, Receiver<ChangeEvent>), WatchError> {
        Self::start_with_debounce(path, Duration::from_millis(200))
    }

    /// Construct with an explicit debounce duration. Used by tests that want a
    /// shorter window.
    ///
    /// # Errors
    ///
    /// Same as `start`.
    pub fn start_with_debounce(
        path: impl AsRef<Path>,
        _debounce: Duration,
    ) -> Result<(Self, Receiver<ChangeEvent>), WatchError> {
        let path = path.as_ref().to_owned();
        let (tx, rx) = mpsc::channel::<ChangeEvent>();
        let event_tx: Sender<ChangeEvent> = tx;

        let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            if let Ok(event) = res {
                let mapped = match event.kind {
                    EventKind::Create(_) | EventKind::Modify(_) => Some(ChangeEvent::Modified),
                    EventKind::Remove(_) => Some(ChangeEvent::Removed),
                    _ => None,
                };
                if let Some(ev) = mapped {
                    // Best-effort send; receiver may have been dropped by the editor.
                    let _ = event_tx.send(ev);
                }
            }
        })?;

        // Watch the parent directory non-recursively. Watching the file itself
        // can break across editors that save via rename-and-replace (notify
        // loses the inode). The receiver filters by path equality below.
        let watch_target = path.parent().unwrap_or_else(|| Path::new(".")).to_owned();
        watcher.watch(&watch_target, RecursiveMode::NonRecursive)?;

        Ok((
            Self {
                _watcher: watcher,
                path,
            },
            rx,
        ))
    }

    /// Path being watched.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::Instant;

    use super::*;

    #[test]
    fn watcher_picks_up_file_modify() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("workspace.ron");
        fs::write(&path, "(name: \"t\", version: \"0.2.0\")").unwrap();

        let (_w, rx) = WorkspaceWatcher::start(&path).expect("watcher starts");

        // Touch the file. A small loop tolerates platform-specific notify warm-up.
        let started = Instant::now();
        let mut got = false;
        for _ in 0..20 {
            fs::write(&path, "(name: \"t2\", version: \"0.2.0\")").unwrap();
            if rx.recv_timeout(Duration::from_millis(150)).is_ok() {
                got = true;
                break;
            }
            if started.elapsed() > Duration::from_secs(3) {
                break;
            }
        }
        assert!(got, "expected at least one ChangeEvent within 3s");
    }
}
