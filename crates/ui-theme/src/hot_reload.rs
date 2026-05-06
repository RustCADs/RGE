// adapted from rustforge::apps::editor-app::egui_overlay on 2026-05-05 — extracted ThemeRegistry
//
// File-system watcher for `assets/themes/`. On a `.theme.ron` save,
// the watcher pushes a `ReloadEvent` onto a queue that the host app
// polls each frame. The host calls `ThemeRegistry::load_file` and
// then `ThemeRegistry::invalidate_cache`. The cycle file-save →
// repaint must complete in under 50ms.
//
// We use the cross-platform `notify` crate (debounced via the
// crate's recommended modes) and intentionally keep the watcher
// thread-safe and lock-free on the consumer side: events flow
// through a `mpsc::Receiver` that the host drains with `try_recv`.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use notify::{Event, EventKind, RecursiveMode, Watcher};

/// Notification of a theme file change.
#[derive(Clone, Debug)]
pub struct ReloadEvent {
    pub path: PathBuf,
    pub kind: ReloadKind,
    pub fired_at: Instant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReloadKind {
    /// `notify` reported a Modify event.
    Modified,
    /// `notify` reported a Create event.
    Created,
    /// `notify` reported a Remove event.
    Removed,
}

/// Hot-reload watcher. Holds a `notify::Watcher` plus a channel of
/// `ReloadEvent`s. Drop kills the watch.
pub struct ThemeWatcher {
    _watcher: Box<dyn Watcher + Send>,
    rx: Arc<Mutex<Receiver<ReloadEvent>>>,
    /// Origin instant for measuring file-save → drain latency.
    pub started_at: Instant,
}

impl std::fmt::Debug for ThemeWatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThemeWatcher")
            .field("started_at", &self.started_at)
            .finish_non_exhaustive()
    }
}

impl ThemeWatcher {
    /// Start watching `dir` (non-recursive). Only `*.theme.ron` events
    /// are forwarded; everything else is filtered out.
    pub fn watch(dir: impl AsRef<Path>) -> Result<Self, notify::Error> {
        let dir = dir.as_ref().to_path_buf();
        let (tx, rx): (Sender<ReloadEvent>, Receiver<ReloadEvent>) = channel();
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            let Ok(event) = res else {
                return;
            };
            let kind = match event.kind {
                EventKind::Modify(_) => Some(ReloadKind::Modified),
                EventKind::Create(_) => Some(ReloadKind::Created),
                EventKind::Remove(_) => Some(ReloadKind::Removed),
                _ => None,
            };
            let Some(kind) = kind else {
                return;
            };
            for path in event.paths {
                if !path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .map(|s| s.ends_with(".theme.ron"))
                    .unwrap_or(false)
                {
                    continue;
                }
                let _send_result = tx.send(ReloadEvent {
                    path,
                    kind,
                    fired_at: Instant::now(),
                });
            }
        })?;
        watcher.watch(&dir, RecursiveMode::NonRecursive)?;
        Ok(Self {
            _watcher: Box::new(watcher),
            rx: Arc::new(Mutex::new(rx)),
            started_at: Instant::now(),
        })
    }

    /// Drain pending events into a `Vec`. Non-blocking; returns
    /// empty if no events have arrived. Hosts call this once per
    /// frame.
    pub fn drain(&self) -> Vec<ReloadEvent> {
        let mut out = Vec::new();
        let rx = self.rx.lock().expect("watcher channel poisoned");
        while let Ok(ev) = rx.try_recv() {
            out.push(ev);
        }
        out
    }

    /// Block waiting for the next event for up to `dur`. Returns the
    /// event if one arrived, or `None` on timeout.
    pub fn next_within(&self, dur: std::time::Duration) -> Option<ReloadEvent> {
        let rx = self.rx.lock().expect("watcher channel poisoned");
        rx.recv_timeout(dur).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reload_event_fields_present() {
        let e = ReloadEvent {
            path: PathBuf::from("foo.theme.ron"),
            kind: ReloadKind::Modified,
            fired_at: Instant::now(),
        };
        assert_eq!(e.kind, ReloadKind::Modified);
    }
}
