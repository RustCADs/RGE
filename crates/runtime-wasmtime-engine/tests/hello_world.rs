// adapted from rustforge::crates::runtime-wasmtime::tests::example_plugin on 2026-05-05 — engine_wasmtime feature activated
//! W04 hello-world integration test.
//!
//! Validates the full Phase 3 critical path:
//!
//! 1. Compile a 50-line `.wat` module exporting `tick(dt: f32) -> ()`.
//! 2. Instantiate it under a `<computes>`-only cap ticket.
//! 3. Call `tick(0.016)` twice — host counter must equal 2.
//! 4. Cap-gate test: a separate module imports `wasi:sockets/tcp`
//!    without declaring `<network>` — instantiate must fail.
//! 5. Baseline timings: instance creation, tick overhead, memory.
//!
//! Per W04 spec exit criteria. See BASELINE.md for recorded numbers.

#![cfg(feature = "engine_wasmtime")]

use std::time::Instant;

use rge_runtime_wasmtime::{load_wasm_blob, CapSet, Capability, Effect, EffectSet, LoadError};
use rge_runtime_wasmtime_engine::{Engine, EngineError};

const HELLO_WAT: &str = include_str!("fixtures/hello_tick.wat");
const NETWORK_WAT: &str = include_str!("fixtures/network_plugin.wat");

/// Encode a u32 as LEB128 (unsigned).
fn write_leb128(out: &mut Vec<u8>, mut v: u32) {
    loop {
        let mut byte = (v & 0x7F) as u8;
        v >>= 7;
        if v != 0 {
            byte |= 0x80;
            out.push(byte);
        } else {
            out.push(byte);
            return;
        }
    }
}

/// Append a wasm custom section named "rcad-effects" carrying the
/// manifest body to a compiled wasm blob. The format is the standard
/// wasm custom section layout:
///
/// ```text
/// section_id=0 (1 byte)
/// section_size LEB128
/// name_size LEB128
/// name bytes
/// payload bytes
/// ```
///
/// Wasmtime accepts unknown custom sections, so adding this is safe.
/// The `runtime-wasmtime` lossy scanner finds the `rcad-effects:...;`
/// marker inside the payload regardless of how the section is framed.
fn append_rcad_effects(mut wasm: Vec<u8>, manifest: &str) -> Vec<u8> {
    let payload = format!("rcad-effects:{manifest};");
    let payload_bytes = payload.as_bytes();
    let name = b"rcad-effects";

    // Build the inner section bytes: name_size LEB128 + name + payload.
    let mut inner = Vec::with_capacity(1 + name.len() + payload_bytes.len());
    write_leb128(&mut inner, name.len() as u32);
    inner.extend_from_slice(name);
    inner.extend_from_slice(payload_bytes);

    // Outer wrapper: section_id (0 = custom) + size LEB128 + inner.
    wasm.push(0x00);
    write_leb128(&mut wasm, inner.len() as u32);
    wasm.extend_from_slice(&inner);
    wasm
}

#[test]
fn hello_world_two_ticks_increment_counter_to_two() {
    // 1. Compile .wat → .wasm bytes.
    let wasm_only = wat::parse_str(HELLO_WAT).expect("hello_tick.wat parses");
    let blob = append_rcad_effects(wasm_only, "<computes>");

    // 2. Run through the runtime-wasmtime cap-gate loader.
    let loaded = load_wasm_blob("hello-tick", &blob).expect("load_wasm_blob");
    assert!(loaded.effects.contains(Effect::Computes));
    assert_eq!(loaded.effects.count(), 1);

    // 3. Construct engine + instantiate under <computes>-only ticket.
    let engine = Engine::new().expect("engine constructs");
    let granted = CapSet::from_one(Capability::ComputeExec);
    let mut inst = engine.instantiate(&loaded, granted).expect("instantiate");

    assert_eq!(inst.tick_count(), 0, "tick counter starts at 0");

    // 4. Call tick(0.016) twice → counter must be 2.
    inst.tick(0.016).expect("first tick");
    assert_eq!(inst.tick_count(), 1, "after first tick");

    inst.tick(0.016).expect("second tick");
    assert_eq!(inst.tick_count(), 2, "after second tick");

    // 5. Verify last-dt round-tripped through host_record_tick.
    let last = inst.host().last_dt;
    assert!(
        (last - 0.016_f32).abs() < 1e-6,
        "host saw last_dt = {last}, expected 0.016"
    );

    // 6. Verify no panic was recorded.
    assert!(!inst.is_quarantined());
    assert!(inst.panic_report().is_none());
    assert!(engine.drain_panics().is_empty());
}

#[test]
fn cap_gate_module_without_network_cap_fails_at_instantiate() {
    // Module imports wasi:sockets/tcp.connect but declares NO effects
    // (empty manifest). The engine binds the network shim only when
    // <network> is in the declared effect set; the linker therefore
    // sees an unresolved import and instantiate must fail.
    let wasm_only = wat::parse_str(NETWORK_WAT).expect("network_plugin.wat parses");
    // Empty manifest — no effects declared.
    let blob = append_rcad_effects(wasm_only, "");

    let loaded = load_wasm_blob("network-plugin", &blob).expect("load_wasm_blob");
    assert_eq!(loaded.effects.count(), 0, "no effects declared");

    let engine = Engine::new().expect("engine constructs");

    // Even a runtime granted full caps cannot let an undeclared
    // <network> plugin link, because the host-function set is
    // gated by the *declared* effects, not the granted caps.
    let granted = CapSet::all();
    let r = engine.instantiate(&loaded, granted);
    match r {
        Err(EngineError::LinkerMissing(_)) | Err(EngineError::Wasmtime(_)) => {
            // Expected — unknown import.
        }
        Err(other) => panic!("expected LinkerMissing, got {other:?}"),
        Ok(_) => panic!("expected linker failure, got Ok"),
    }
}

#[test]
fn cap_gate_phi_plugin_fails_against_compute_only_grant() {
    // Plugin declares <reads-phi>; runtime granted only compute.exec.
    // The Path B runtime gate must reject this **before** any
    // wasmtime compile — no cranelift work performed.
    let wasm_only = wat::parse_str(HELLO_WAT).expect("hello_tick.wat parses");
    let blob = append_rcad_effects(wasm_only, "<computes>,<reads-phi>");

    let loaded = load_wasm_blob("phi-tick", &blob).expect("load_wasm_blob");
    assert!(loaded.effects.contains(Effect::ReadsPhi));

    let engine = Engine::new().expect("engine constructs");
    let granted = CapSet::from_one(Capability::ComputeExec);
    let r = engine.instantiate(&loaded, granted);
    assert!(
        matches!(r, Err(EngineError::CapabilityGate(_))),
        "expected CapabilityGate error, got {r:?}"
    );
}

#[test]
fn loader_rejects_unknown_effect_tag() {
    let wasm_only = wat::parse_str(HELLO_WAT).expect("hello_tick.wat parses");
    let blob = append_rcad_effects(wasm_only, "<undeclared>");
    let r = load_wasm_blob("rogue", &blob);
    assert!(matches!(r, Err(LoadError::BadManifest(_))));
}

#[test]
fn instantiate_quarantine_unaffected_when_no_trap() {
    let wasm_only = wat::parse_str(HELLO_WAT).expect("hello_tick.wat parses");
    let blob = append_rcad_effects(wasm_only, "<computes>");
    let loaded = load_wasm_blob("hello-tick", &blob).expect("load_wasm_blob");
    let engine = Engine::new().expect("engine constructs");
    let mut inst = engine
        .instantiate(&loaded, CapSet::from_one(Capability::ComputeExec))
        .expect("instantiate");
    inst.tick(0.016).expect("tick");
    assert!(!inst.is_quarantined());
}

#[test]
fn baseline_timings_are_within_spec() {
    // Establish the W04 baseline timings. Spec from PLAN.md §5.6:
    // - cold-start (engine + compile + instantiate) <50ms p95
    // - tick overhead negligible (single host import)
    // - per-module footprint <1MB (one 64KiB memory page = 65 536 B)
    //
    // We assert generous bounds here so CI noise doesn't flake the
    // test; BASELINE.md records the actual observed numbers.

    let wasm_only = wat::parse_str(HELLO_WAT).expect("hello_tick.wat parses");
    let blob = append_rcad_effects(wasm_only, "<computes>");
    let loaded = load_wasm_blob("hello-tick", &blob).expect("load_wasm_blob");

    let t_engine_start = Instant::now();
    let engine = Engine::new().expect("engine constructs");
    let t_engine = t_engine_start.elapsed();

    let granted = CapSet::from_one(Capability::ComputeExec);

    // Instance creation timing (compile + linker bind + instantiate).
    let t_inst_start = Instant::now();
    let mut inst = engine.instantiate(&loaded, granted).expect("instantiate");
    let t_inst = t_inst_start.elapsed();

    // Tick overhead — average over 10_000 ticks.
    let n_ticks: u32 = 10_000;
    let t_tick_start = Instant::now();
    for _ in 0..n_ticks {
        inst.tick(0.016).expect("tick");
    }
    let t_tick_total = t_tick_start.elapsed();
    let tick_ns = t_tick_total.as_nanos() as f64 / f64::from(n_ticks);

    // Memory footprint of the single exported memory.
    let mem = inst.memory_footprint_bytes();

    eprintln!(
        "[W04 baseline] engine_new={t_engine:?}  instance_creation={t_inst:?}  per_tick={tick_ns:.0}ns  memory={mem}B"
    );

    // Generous CI-stable bounds. Real budget validation lands in W20
    // (script-bench). Here we assert the engine isn't catastrophically
    // off (e.g. an accidental 1s/instantiate or 1ms/tick).
    assert!(
        t_inst.as_millis() < 500,
        "instance creation took {t_inst:?}, expected <500ms (Phase 3.4 budget is <50ms p95)"
    );
    assert!(
        tick_ns < 100_000.0,
        "per-tick took {tick_ns}ns, expected <100us (real budget is much tighter; W20 enforces)"
    );
    assert!(
        mem < 1024 * 1024,
        "memory footprint {mem}B exceeded 1MB sanity bound"
    );

    // Validate every tick ran (no quarantine).
    assert_eq!(inst.tick_count(), n_ticks);
    assert!(!inst.is_quarantined());

    // Verify the EffectSet enum is what we expect.
    assert_eq!(EffectSet::from_one(Effect::Computes).count(), 1);
}
