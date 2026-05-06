// adapted from rustforge::apps::editor-app::app_lifecycle on 2026-05-05 — PlayState transitions added
//
//! `EditorShell` — the editor host that owns winit's `ApplicationHandler`,
//! the PIE state machine, and the world/snapshot/audit-ledger triad.
//!
//! Per W03 dispatch and PLAN.md §6.13. Adapted from
//! `rustforge/apps/editor-app/src/app_lifecycle.rs`. The original drives a
//! single editor app with no PIE concept — its `RedrawRequested` always
//! ticks game systems. RGE's `EditorShell` adds:
//!
//! - [`PlayState`] gating: `RedrawRequested` only ticks game systems when
//!   `state.game_systems_run()` returns `true`.
//! - [`WorldSnapshot`] capture on `[Play]`, restore on `[Stop]`.
//! - [`TimeScale`] applied to the per-tick `dt` for game systems.
//! - [`PlayToolbar`] wired through [`Self::handle_button`].
//!
//! The original rustforge file pulls in wgpu device/queue/pipeline state
//! and an egui overlay; W03 strips those out (gfx wave W21+ owns wgpu)
//! and keeps only the lifecycle skeleton + PIE plumbing. Window creation
//! is also stubbed — `resumed` allocates the [`Viewport`] but does not
//! create a winit window (the real `editor/rge-editor` binary will own
//! that and forward events to `EditorShell`).

use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

use crate::audit::{AuditEvent, AuditLedger};
use crate::coord::EditorCoord;
use crate::play_state::{PlayState, PlayStateError, PlayStateTransition};
use crate::play_toolbar::{PlayToolbar, ToolbarButtonId};
use crate::snapshot::{capture_and_audit, restore_and_audit, WorldSnapshot};
use crate::time_scale::{TimeScale, TimeScaleClass};
use crate::viewport::Viewport;
use crate::world::World;

/// Default progress-line interval (frames). Mirrors rustforge's
/// `PROGRESS_FRAME_INTERVAL` — once per ~second at 60Hz.
const PROGRESS_FRAME_INTERVAL: u64 = 60;

/// The editor host. Owns:
///
/// - the live `World` (authoritative runtime state during Editing; mutable
///   during Playing; restored on Stop)
/// - the editor coordination state (`EditorCoord`) — *never* in the
///   snapshot, so it persists across Play/Stop (PLAN.md §1.15)
/// - the `PlayState` machine
/// - the optional captured snapshot (`Some` while in PIE, `None` in Editing)
/// - the play-mode toolbar registration
/// - the time-scale setting
/// - the placeholder viewport widget
/// - the audit ledger for PIE events
///
/// Lifecycle (winit 0.30 `ApplicationHandler`):
///
/// ```text
/// resumed       — first call: allocate Viewport, log "ready" banner.
///                 Idempotent on re-resume (mobile suspend/resume).
/// window_event  — `RedrawRequested` drives one tick (game systems gated
///                 by PlayState); `CloseRequested` exits the loop.
/// suspended     — drop transient widget state; preserve PIE snapshot
///                 (so resume-from-suspend in Playing keeps the round-trip
///                 viable).
/// ```
pub struct EditorShell {
    world: World,
    coord: EditorCoord,
    state: PlayState,
    snapshot: Option<WorldSnapshot>,
    toolbar: PlayToolbar,
    time_scale: TimeScale,
    viewport: Viewport,
    audit: AuditLedger,
    /// Total ticks executed (game-system ticks; pumped by Redraw +
    /// `PlayState`). Used for diagnostics + the audit-log capture-tick field.
    tick_count: u64,
    /// Last frame's wall-clock instant. Real schedule-driver maintains a
    /// running accumulator (W04+); W03 stages the field.
    last_frame_instant: Option<Instant>,
    /// Whether `resumed()` has run at least once. winit allows multiple
    /// resume callbacks (mobile); we treat the second as a no-op for the
    /// fields that have already been initialized.
    initialized: bool,
}

impl EditorShell {
    /// Construct a fresh shell with an empty world.
    #[must_use]
    pub fn new() -> Self {
        Self::with_world(World::new())
    }

    /// Construct with a pre-populated world (used by tests and by the
    /// `editor/rge-editor` binary's scene-load path).
    #[must_use]
    pub fn with_world(world: World) -> Self {
        Self {
            world,
            coord: EditorCoord::new(),
            state: PlayState::default(),
            snapshot: None,
            toolbar: PlayToolbar::standard(),
            time_scale: TimeScale::default(),
            viewport: Viewport::default(),
            audit: AuditLedger::default(),
            tick_count: 0,
            last_frame_instant: None,
            initialized: false,
        }
    }

    // ---- accessors (read-only) ---------------------------------------------

    /// Borrow the live world (mutable access exposed for tests / scene-load).
    #[must_use]
    pub fn world(&self) -> &World {
        &self.world
    }

    /// Mutable world access. Real editors funnel mutations through the
    /// Command Bus (PLAN.md §6.16); W03 leaves direct access for the
    /// integration test that builds the 100-entity scene.
    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    /// Current `PlayState`.
    #[must_use]
    pub fn play_state(&self) -> PlayState {
        self.state
    }

    /// Borrow the editor coordination state.
    #[must_use]
    pub fn coord(&self) -> &EditorCoord {
        &self.coord
    }

    /// Mutable editor-coord access (selection updates land here).
    pub fn coord_mut(&mut self) -> &mut EditorCoord {
        &mut self.coord
    }

    /// Borrow the play-mode toolbar.
    #[must_use]
    pub fn toolbar(&self) -> &PlayToolbar {
        &self.toolbar
    }

    /// Current time-scale.
    #[must_use]
    pub fn time_scale(&self) -> TimeScale {
        self.time_scale
    }

    /// Borrow the audit ledger (read-only; tests assert event sequence).
    #[must_use]
    pub fn audit(&self) -> &AuditLedger {
        &self.audit
    }

    /// Borrow the placeholder viewport.
    #[must_use]
    pub fn viewport(&self) -> &Viewport {
        &self.viewport
    }

    /// Total game-system ticks executed since shell construction.
    #[must_use]
    pub fn tick_count(&self) -> u64 {
        self.tick_count
    }

    /// Whether a snapshot is currently held (i.e. in PIE).
    #[must_use]
    pub fn has_snapshot(&self) -> bool {
        self.snapshot.is_some()
    }

    // ---- toolbar entry points ----------------------------------------------

    /// Dispatch a toolbar-button press. Returns the resulting transition,
    /// or `Err` if the press was rejected by the state machine. The
    /// integration test asserts the exact transition sequence; the real
    /// UI swallows errors silently (disabled buttons should never have
    /// fired, but the state machine is the authoritative gate).
    ///
    /// # Errors
    ///
    /// Returns [`PlayStateError`] when the button press is invalid for the
    /// current [`PlayState`] (e.g. pressing Stop while in Editing).
    ///
    /// # Panics
    ///
    /// Panics if the internal snapshot invariant is violated (i.e.
    /// `StoppedAndRestored` is returned without a snapshot being held).
    pub fn handle_button(
        &mut self,
        id: ToolbarButtonId,
    ) -> Result<PlayStateTransition, PlayStateError> {
        match id {
            ToolbarButtonId::Play => {
                let before = self.state;
                let t = self.state.play()?;
                if t == PlayStateTransition::StartedPlay {
                    // Capture the snapshot at the moment of Play.
                    let snap = capture_and_audit(&self.world, self.tick_count, &mut self.audit);
                    self.snapshot = Some(snap);
                }
                self.audit.record(AuditEvent::PlayPressed {
                    before_state: before.label(),
                });
                Ok(t)
            }
            ToolbarButtonId::Pause => {
                let t = self.state.pause()?;
                self.audit.record(AuditEvent::PausePressed);
                Ok(t)
            }
            ToolbarButtonId::Stop => {
                let t = self.state.stop()?;
                if t == PlayStateTransition::StoppedAndRestored {
                    let snap = self
                        .snapshot
                        .take()
                        .expect("StoppedAndRestored implies snapshot was held");
                    restore_and_audit(&snap, &mut self.world, &mut self.audit);
                }
                self.audit.record(AuditEvent::StopPressed);
                Ok(t)
            }
            ToolbarButtonId::Step => {
                let t = self.state.step()?;
                self.audit.record(AuditEvent::StepPressed);
                // Step advances one game tick at the configured scale,
                // *bypassing* the PlayState gate (Step is the explicit
                // "tick once even though Paused" affordance).
                self.advance_game_tick(default_dt());
                Ok(t)
            }
            ToolbarButtonId::FrameStep => {
                let t = self.state.frame_step()?;
                self.audit.record(AuditEvent::FrameStepPressed);
                // FrameStep is "advance one render frame". W03 stages it as
                // a tick advance equal to one frame at 60Hz; W04 will
                // diverge tick from frame via the schedule accumulator.
                self.advance_game_tick(default_dt());
                Ok(t)
            }
        }
    }

    /// Adjust the time-scale slider. Records a [`AuditEvent::TimeScaleChanged`]
    /// audit event with the from/to values.
    pub fn set_time_scale(&mut self, value: f32) {
        let from = self.time_scale.value();
        let prev = self.time_scale.set(value);
        debug_assert!(
            (prev - from).abs() < 1e-9,
            "TimeScale::set returned previous != self.value()"
        );
        self.audit.record(AuditEvent::TimeScaleChanged {
            from,
            to: self.time_scale.value(),
        });
    }

    /// Advance one game-system tick, applying the configured time-scale.
    /// Editor systems are not invoked here (they run unconditionally on
    /// every redraw, regardless of `PlayState` — PLAN.md constitutional
    /// principle #8).
    fn advance_game_tick(&mut self, dt_seconds: f32) {
        let scaled = self.time_scale.apply(dt_seconds, TimeScaleClass::Game);
        self.world.tick_game_systems(scaled);
        self.tick_count += 1;
    }

    /// Tick the schedule for one redraw. Internal — invoked from
    /// `window_event::RedrawRequested`, but exposed `pub(crate)` so the
    /// integration test can drive ticks without spinning a real winit
    /// event loop.
    pub fn tick_redraw(&mut self) {
        // 1) Update wall-clock dt (real schedule-accumulator wave will
        //    refine this; W03 fixes 1/60 = 16.67ms).
        let dt = default_dt();
        self.last_frame_instant = Some(Instant::now());

        // 2) Game systems run only when PlayState says so.
        if self.state.game_systems_run() {
            self.advance_game_tick(dt);
        }

        // 3) Editor systems always run. W03 has no editor systems yet; the
        //    only "editor side-effect" is updating the viewport overlay.
        self.viewport.update_overlay(self.state, self.time_scale);

        // 4) Diagnostic progress line at the rustforge interval.
        if self.tick_count > 0 && self.tick_count % PROGRESS_FRAME_INTERVAL == 0 {
            tracing::trace!(
                target: "rge::editor-shell::lifecycle",
                tick = self.tick_count,
                state = self.state.label(),
                scale = self.time_scale.value(),
                "tick"
            );
        }
    }

    /// Drive `n` redraws in a tight loop. Used by the round-trip
    /// integration test (60-tick run between Play and Stop).
    pub fn run_for_redraws(&mut self, n: u64) {
        for _ in 0..n {
            self.tick_redraw();
        }
    }

    // ---- diagnostics --------------------------------------------------------

    /// Compose a one-line readiness banner (rustforge pattern).
    fn ready_banner(&self) -> String {
        format!(
            "rge-editor-shell: ready — viewport {}x{} state={} scale=×{:.2}",
            self.viewport.width(),
            self.viewport.height(),
            self.state.label(),
            self.time_scale.value(),
        )
    }
}

impl Default for EditorShell {
    fn default() -> Self {
        Self::new()
    }
}

/// Default frame-time for ticks (60Hz). Real schedule-accumulator (W04+)
/// will compute this from wall clock; W03 fixes the value so the
/// round-trip test is deterministic across machines.
///
/// Not `const` because Rust 1.78 does not allow FP arithmetic in const
/// functions (see rust-lang issue #57241); the literal value is
/// trivially inlinable by LLVM regardless.
fn default_dt() -> f32 {
    1.0 / 60.0
}

// -------------------------------------------------------------------------
// winit ApplicationHandler — the event-loop entry surface
// -------------------------------------------------------------------------

impl ApplicationHandler<()> for EditorShell {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        // adapted from rustforge::apps::editor-app::app_lifecycle on 2026-05-05
        //   — wgpu/window-construction stripped (W21+ owns those); we keep
        //     the idempotent re-resume guard.
        if self.initialized {
            return;
        }
        // Real `editor/rge-editor` binary creates the window via
        // `event_loop.create_window(...)`; W03 keeps the viewport at its
        // default size. Wiring the window handle in is W08+'s job (the
        // editor-shell needs egui_dock/wgpu alive before the window is
        // useful).
        tracing::info!(
            target: "rge::editor-shell::lifecycle",
            "{}",
            self.ready_banner()
        );
        self.initialized = true;
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        // adapted from rustforge::apps::editor-app::app_lifecycle on 2026-05-05
        //   — egui-overlay routing + IR-rebuild + close-persist stripped.
        //     PIE-aware tick driver replaces the rustforge unconditional
        //     `app.run_for_ticks(1)` call.
        match event {
            WindowEvent::CloseRequested => {
                tracing::info!(
                    target: "rge::editor-shell::lifecycle",
                    ticks = self.tick_count,
                    "close requested"
                );
                event_loop.exit();
            }
            WindowEvent::Resized(new_size) => {
                self.viewport.resize(new_size.width, new_size.height);
            }
            WindowEvent::RedrawRequested => {
                self.tick_redraw();
            }
            _ => {}
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        // Mobile-style suspend: drop transient widget state but PRESERVE
        // any in-flight PIE snapshot — resuming from suspend should leave
        // PIE viable. The `initialized` flag is reset so `resumed` rebuilds
        // the viewport.
        tracing::info!(
            target: "rge::editor-shell::lifecycle",
            "suspended (PIE snapshot preserved={})",
            self.snapshot.is_some()
        );
        self.initialized = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::ComponentTypeId;

    fn build_scene(shell: &mut EditorShell, n: usize) {
        for i in 0..n {
            let e = shell.world_mut().spawn();
            shell.world_mut().insert_component(
                e,
                ComponentTypeId(1),
                (i as u64).to_le_bytes().to_vec(),
            );
            shell
                .world_mut()
                .insert_component(e, ComponentTypeId(2), vec![0u8; 12]);
        }
    }

    #[test]
    fn fresh_shell_is_editing() {
        let s = EditorShell::new();
        assert_eq!(s.play_state(), PlayState::Editing);
        assert!(!s.has_snapshot());
        assert_eq!(s.tick_count(), 0);
    }

    #[test]
    fn play_button_captures_snapshot() {
        let mut s = EditorShell::new();
        build_scene(&mut s, 5);
        let t = s.handle_button(ToolbarButtonId::Play).unwrap();
        assert_eq!(t, PlayStateTransition::StartedPlay);
        assert!(s.has_snapshot());
        assert_eq!(s.play_state(), PlayState::Playing);
    }

    #[test]
    fn editing_does_not_tick_game_systems() {
        let mut s = EditorShell::new();
        build_scene(&mut s, 5);
        let pre = s.world().serialize();
        s.run_for_redraws(10);
        let post = s.world().serialize();
        assert_eq!(pre, post, "Editing must not advance game state");
        assert_eq!(s.tick_count(), 0);
    }

    #[test]
    fn playing_advances_game_systems() {
        let mut s = EditorShell::new();
        build_scene(&mut s, 5);
        s.handle_button(ToolbarButtonId::Play).unwrap();
        let pre = s.world().serialize();
        s.run_for_redraws(10);
        let post = s.world().serialize();
        assert_ne!(pre, post, "Playing must advance game state");
        assert_eq!(s.tick_count(), 10);
    }

    #[test]
    fn stop_restores_snapshot() {
        let mut s = EditorShell::new();
        build_scene(&mut s, 10);
        let pre_play = s.world().serialize();
        s.handle_button(ToolbarButtonId::Play).unwrap();
        s.run_for_redraws(60);
        let mid = s.world().serialize();
        assert_ne!(pre_play, mid);
        s.handle_button(ToolbarButtonId::Stop).unwrap();
        let post_stop = s.world().serialize();
        assert_eq!(pre_play, post_stop, "byte-identical restore");
        assert!(!s.has_snapshot());
        assert_eq!(s.play_state(), PlayState::Editing);
    }

    #[test]
    fn pause_freezes_game_systems() {
        let mut s = EditorShell::new();
        build_scene(&mut s, 5);
        s.handle_button(ToolbarButtonId::Play).unwrap();
        s.run_for_redraws(5);
        let mid = s.world().serialize();
        s.handle_button(ToolbarButtonId::Pause).unwrap();
        s.run_for_redraws(20);
        let after_pause = s.world().serialize();
        assert_eq!(mid, after_pause, "Paused must freeze game state");
    }

    #[test]
    fn step_advances_one_tick_in_paused() {
        let mut s = EditorShell::new();
        build_scene(&mut s, 5);
        s.handle_button(ToolbarButtonId::Play).unwrap();
        s.handle_button(ToolbarButtonId::Pause).unwrap();
        let pre = s.world().serialize();
        let pre_count = s.tick_count();
        s.handle_button(ToolbarButtonId::Step).unwrap();
        let post = s.world().serialize();
        assert_ne!(pre, post, "Step must advance one tick");
        assert_eq!(s.tick_count(), pre_count + 1);
    }

    #[test]
    fn step_invalid_in_editing() {
        let mut s = EditorShell::new();
        let result = s.handle_button(ToolbarButtonId::Step);
        assert!(result.is_err());
    }

    #[test]
    fn time_scale_affects_game_only() {
        let mut s = EditorShell::new();
        let e = s.world_mut().spawn();
        s.world_mut()
            .insert_component(e, ComponentTypeId(2), vec![0u8; 12]);
        s.set_time_scale(2.0);
        s.handle_button(ToolbarButtonId::Play).unwrap();
        s.run_for_redraws(60);
        let p = s.world().component(e, ComponentTypeId(2)).unwrap().clone();
        let mut x_bytes = [0u8; 4];
        x_bytes.copy_from_slice(&p[0..4]);
        let x = f32::from_le_bytes(x_bytes);
        // Position increments by `dt_scaled` per tick; with scale=2 and
        // dt=1/60 across 60 ticks, x = 60 * (1/60) * 2 = 2.0
        assert!((x - 2.0).abs() < 1e-3, "expected ~2.0, got {x}");
    }

    #[test]
    fn audit_records_play_stop() {
        let mut s = EditorShell::new();
        build_scene(&mut s, 5);
        s.handle_button(ToolbarButtonId::Play).unwrap();
        s.handle_button(ToolbarButtonId::Stop).unwrap();
        let tags: Vec<_> = s.audit().iter().map(AuditEvent::tag).collect();
        assert!(tags.contains(&"SnapshotCaptured"));
        assert!(tags.contains(&"PlayPressed"));
        assert!(tags.contains(&"SnapshotRestored"));
        assert!(tags.contains(&"StopPressed"));
    }
}
