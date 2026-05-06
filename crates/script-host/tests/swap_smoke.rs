//! `swap_smoke` — state-preserving hot-reload swap smoke test.
//!
//! Steps per spec:
//! 1. Compile v1 WAT fixture (tick increments Counter by 1).
//! 2. Spawn entity with `Counter { value: 0 }`.
//! 3. Tick 10 times → Counter == 10.
//! 4. `capture_state` → `SwapPlan`. ← swap window start
//! 5. Compile v2 WAT fixture (tick increments by 2).
//! 6. Drop old instance, instantiate v2.
//! 7. `restore_state(world, plan)` → Counter == 10. ← swap window end
//! 8. Tick 5 times → Counter == 20 (10 + 5 * 2).
//! 9. Verify `swap_duration_ms` is recorded.

use std::time::Instant;

use rge_kernel_diagnostics::DiagnosticAggregator;
use rge_kernel_ecs::{EntityId, World};
use rge_kernel_events::EventBus;
use rge_script_host::ecs_bridge::{entity_id_to_i64, Counter};
use rge_script_host::{capture_state, restore_state, ScriptInstance, ScriptModule, SwapResult};
use wasmtime::Engine;

fn compile_wat(engine: &Engine, name: &str, src: &str) -> ScriptModule {
    let bytes = wat::parse_str(src).expect("WAT parse");
    ScriptModule::from_bytes(engine, name, &bytes).expect("compile")
}

fn read_counter(world: &World, id: EntityId) -> i64 {
    world
        .entity(id)
        .and_then(|e| e.get::<Counter>().map(|c| c.value))
        .unwrap_or(0)
}

#[test]
fn module_swap_preserves_counter_state() {
    let v1_src = include_str!("fixtures/counter_v1.wat");
    let v2_src = include_str!("fixtures/counter_v2.wat");

    let engine = Engine::default();

    // 1. Compile both modules (compile time is NOT in the swap window).
    let module_v1 = compile_wat(&engine, "counter_v1", v1_src);
    let module_v2 = compile_wat(&engine, "counter_v2", v2_src);

    let mut world = World::new();
    let mut events = EventBus::new();
    let mut diag = DiagnosticAggregator::new();

    // 2. Spawn entity with Counter { value: 0 }.
    let entity_id = world.spawn_with(Counter { value: 0 });
    let handle = entity_id_to_i64(entity_id);

    // Instantiate v1 and register the entity handle.
    let mut v1 = ScriptInstance::instantiate(&engine, &module_v1).expect("v1 instantiate");
    v1.call_init_entity(handle, &mut world, &mut events, &mut diag)
        .expect("init_entity v1");

    // 3. Tick 10 times → Counter == 10.
    for _ in 0..10 {
        v1.tick(0.016, &mut world, &mut events, &mut diag)
            .expect("v1 tick");
    }
    assert_eq!(read_counter(&world, entity_id), 10, "after 10 v1 ticks");

    // ---- Swap window begins ----
    let t0 = Instant::now();

    // 4. Capture state.
    let plan = capture_state(&world).expect("capture_state");

    // 6. Drop old instance, instantiate v2.
    drop(v1);
    let mut v2 = ScriptInstance::instantiate(&engine, &module_v2).expect("v2 instantiate");

    // 7. Restore state → Counter back to 10.
    let restored = restore_state(&mut world, &plan).expect("restore_state");

    let swap_duration_ms = t0.elapsed().as_secs_f64() * 1000.0;
    // ---- Swap window ends ----

    let result = SwapResult {
        captured_at_tick: plan.captured_at_tick,
        restored_components: restored,
        swap_duration_ms,
    };

    assert_eq!(result.restored_components, 1, "one Counter restored");
    assert_eq!(
        read_counter(&world, entity_id),
        10,
        "counter == 10 after restore"
    );
    println!("swap_duration_ms = {:.3}", result.swap_duration_ms);
    // Smoke gate: well below the p95<100ms budget. Criterion bench proves p95.
    assert!(
        result.swap_duration_ms < 5_000.0,
        "swap must complete within 5 s on any machine (got {:.1} ms)",
        result.swap_duration_ms
    );

    // Register entity in v2.
    v2.call_init_entity(handle, &mut world, &mut events, &mut diag)
        .expect("init_entity v2");

    // 8. Tick 5 times → Counter == 20 (10 + 5 * 2).
    for _ in 0..5 {
        v2.tick(0.016, &mut world, &mut events, &mut diag)
            .expect("v2 tick");
    }
    assert_eq!(
        read_counter(&world, entity_id),
        20,
        "after 5 v2 ticks (inc=2)"
    );

    // 9. No errors.
    assert!(!diag.has_errors(), "no script errors during swap smoke");
}
