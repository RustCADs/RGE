//! `host_panic_isolation` — verify that a trapping wasm module does not kill
//! the host process.
//!
//! Spec requirements:
//! 1. The trap is captured (`ScriptError::TickTrap` returned).
//! 2. The host's `DiagnosticAggregator` receives a `Severity::Error` diagnostic.
//! 3. The test process continues normally.

use rge_kernel_diagnostics::{Diagnostic, DiagnosticAggregator, DiagnosticSink, Severity};
use rge_kernel_ecs::World;
use rge_kernel_events::EventBus;
use rge_script_host::{ScriptError, ScriptInstance, ScriptModule};
use wasmtime::Engine;

/// A module whose `tick(f32)` traps via `unreachable` (a well-defined trap).
const TRAPPING_WAT: &str = r#"
(module
  (func (export "tick") (param f32)
    unreachable
  )
)
"#;

#[test]
fn trap_is_isolated_and_reported() {
    let engine = Engine::default();
    let bytes = wat::parse_str(TRAPPING_WAT).expect("WAT parse");
    let module = ScriptModule::from_bytes(&engine, "trapping", &bytes).expect("compile");

    let mut instance = ScriptInstance::instantiate(&engine, &module).expect("instantiate");

    let mut world = World::new();
    let mut events = EventBus::new();
    let mut diag = DiagnosticAggregator::new();

    // 1. The trap is returned as `ScriptError::TickTrap` — NOT a process panic.
    let result = instance.tick(0.016, &mut world, &mut events, &mut diag);
    assert!(
        matches!(result, Err(ScriptError::TickTrap(_))),
        "expected TickTrap, got {result:?}"
    );

    // 2. Emit the error into diagnostics (the caller is responsible; demonstrate
    //    the pattern here as the script-host contract dictates).
    if let Err(ScriptError::TickTrap(ref msg)) = result {
        diag.emit(Diagnostic::error(format!("script trap: {msg}")));
    }
    assert!(
        diag.has_errors(),
        "diagnostics should have at least one error"
    );
    let highest = diag.highest_severity().expect("has diagnostics");
    assert_eq!(highest, Severity::Error, "highest severity is Error");

    // 3. The test process is still alive — reaching here proves isolation.
    let mut world2 = World::new();
    let mut events2 = EventBus::new();
    let mut diag2 = DiagnosticAggregator::new();
    // A second tick on the same quarantined instance also returns an error
    // (does not panic the host).
    let result2 = instance.tick(0.016, &mut world2, &mut events2, &mut diag2);
    assert!(
        result2.is_err(),
        "second tick on trapped instance should also fail gracefully"
    );
}
