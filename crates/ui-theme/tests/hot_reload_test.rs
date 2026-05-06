// adapted from rustforge::apps::editor-app::egui_overlay on 2026-05-05 — extracted ThemeRegistry
//
// Hot-reload latency test: writing to a `.theme.ron` file in a watched
// directory must surface a `ReloadEvent` within 50ms.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use rge_ui_theme::hot_reload::{ReloadKind, ThemeWatcher};
use rge_ui_theme::{Theme, ThemeRegistry, Token};

fn write_theme(path: &PathBuf, body: &str) {
    fs::write(path, body).unwrap();
}

fn dark_template(version_marker: u32) -> String {
    format!(
        r#"(
    name: "live-test",
    version: {version_marker},
    extends: None,
    variants: [],
    tokens: {{
        "color.background":
            Color((srgb: (10, 10, 10, 255), linear: (0.001, 0.001, 0.001, 1.0))),
        "color.foreground":
            Color((srgb: (240, 240, 240, 255), linear: (0.871, 0.871, 0.871, 1.0))),
    }},
    styles: {{}},
)
"#,
        version_marker = if version_marker == 0 {
            1
        } else {
            version_marker
        }
    )
}

#[test]
fn watcher_fires_on_file_save() {
    let dir = tempfile::tempdir().unwrap();
    let path: PathBuf = dir.path().join("live-test.theme.ron");
    write_theme(&path, &dark_template(1));

    let watcher = ThemeWatcher::watch(dir.path()).expect("watcher start");
    // Tiny delay to let the platform finish wiring up.
    std::thread::sleep(Duration::from_millis(20));

    // Trigger a "save" by rewriting the file.
    write_theme(&path, &dark_template(1));

    // Wait up to 1 second total for the OS event (on Windows the
    // ReadDirectoryChangesW wakeup can be ~200ms even though our
    // <50ms target is the *handling* latency).
    let event = watcher.next_within(Duration::from_secs(1));
    assert!(event.is_some(), "watcher did not see save");
    let ev = event.unwrap();
    assert_eq!(ev.kind, ReloadKind::Modified);
}

#[test]
fn reload_handle_under_50ms() {
    // Measure the registry-side hot-reload latency: from the moment
    // the host receives the event to a successful merged() rebuild.
    // This is the metric called out in the dispatch package.
    let dir = tempfile::tempdir().unwrap();
    let path: PathBuf = dir.path().join("live-test.theme.ron");
    write_theme(&path, &dark_template(1));

    let mut registry = ThemeRegistry::new();
    registry.load_dir(dir.path()).unwrap();
    registry.set_active("live-test").unwrap();
    let _ = registry.merged().unwrap();

    // Simulate "file save → handle event":
    let new_body = r#"(
    name: "live-test",
    version: 1,
    extends: None,
    variants: [],
    tokens: {
        "color.background":
            Color((srgb: (50, 50, 50, 255), linear: (0.031, 0.031, 0.031, 1.0))),
        "color.foreground":
            Color((srgb: (220, 220, 220, 255), linear: (0.695, 0.695, 0.695, 1.0))),
    },
    styles: {},
)"#;
    fs::write(&path, new_body).unwrap();

    let start = Instant::now();
    registry.load_file(&path).unwrap();
    registry.invalidate_cache();
    let merged = registry.merged().unwrap();
    let elapsed = start.elapsed();

    if let Token::Color(c) = merged.tokens.get("color.background").unwrap() {
        assert_eq!(c.srgb[0], 50);
    } else {
        panic!();
    }
    assert!(
        elapsed < Duration::from_millis(50),
        "hot-reload handle took {:?}, target <50ms",
        elapsed
    );
}

#[test]
fn watcher_filters_non_theme_files() {
    let dir = tempfile::tempdir().unwrap();
    let watcher = ThemeWatcher::watch(dir.path()).unwrap();
    std::thread::sleep(Duration::from_millis(20));
    // Write a file that's NOT *.theme.ron — must be filtered out.
    fs::write(dir.path().join("unrelated.txt"), "junk").unwrap();
    let ev = watcher.next_within(Duration::from_millis(200));
    assert!(ev.is_none(), "non-theme.ron file leaked through filter");
}

#[test]
fn registry_load_file_round_trip_into_theme() {
    let dir = tempfile::tempdir().unwrap();
    let path: PathBuf = dir.path().join("rt.theme.ron");
    let mut t = Theme::new("rt");
    t.set_token(
        "color.x",
        Token::Color(rge_ui_theme::Color::from_srgb(1, 2, 3)),
    );
    fs::write(&path, t.to_ron_pretty().unwrap()).unwrap();
    let mut r = ThemeRegistry::new();
    r.load_file(&path).unwrap();
    assert!(r.get("rt").is_some());
}
