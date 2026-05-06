;; W04 hello-world fixture — exports `tick(dt: f32) -> ()` which calls
;; the host-bound `host_record_tick(dt)` import (gated by <computes>).
;;
;; Adapted from the design in rustforge::runtime-wasmtime/tests on
;; 2026-05-05 — engine_wasmtime feature activated.
;;
;; The host-side counter increments by 1 per tick. After two
;; `tick(0.016)` calls the host's `tick_counter` should equal 2.
;;
;; The rcad-effects manifest is appended by the test harness as a
;; proper wasm custom section (LEB128-framed) — see `hello_world.rs`
;; `append_rcad_effects_custom_section`.

(module
  (import "host" "host_record_tick" (func $host_record_tick (param f32)))

  (memory (export "memory") 1)

  (func $tick (export "tick") (param $dt f32)
    (call $host_record_tick (local.get $dt))
  )
)
