;; counter_bulk.wat — fixture for Phase 3.4 ECS-via-WASM ratio gate.
;;
;; Imports: rge.ecs::add_to_all_counters.
;; Behaviour: tick(dt) increments every Counter-bearing entity by 1
;;            in a single host call, replacing the v1/v2 per-entity
;;            init_entity + get + set protocol with one bulk call per
;;            frame. Used only by the ratio measurement; v1/v2 remain
;;            the hot-reload preservation gate fixtures.

(module
  (import "rge.ecs" "add_to_all_counters" (func $add_all (param i64) (result i64)))

  (func (export "tick") (param $dt f32)
    (drop (call $add_all (i64.const 1)))
  )
)
