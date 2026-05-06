;; counter_v1.wat — fixture for swap_smoke test.
;;
;; Imports: rge.ecs::entity_count, get_counter, set_counter.
;; Behaviour: tick() iterates entities (using entity_count) and increments
;;            each Counter by 1 per call.
;;
;; Protocol:
;;   - The test pre-spawns ONE entity with Counter { value: 0 } (handle passed
;;     via a global the host writes via `init_entity`).
;;   - tick(dt) reads the counter, adds 1, writes it back.
;;
;; We expose an `init_entity(handle: i64)` export so the test can tell the
;; wasm module which entity handle to operate on.

(module
  ;; --- imports ---
  (import "rge.ecs" "entity_count" (func $entity_count (result i64)))
  (import "rge.ecs" "get_counter"  (func $get_counter  (param i64) (result i64)))
  (import "rge.ecs" "set_counter"  (func $set_counter  (param i64 i64) (result i32)))

  ;; --- state ---
  (global $entity_handle (mut i64) (i64.const -1))

  ;; --- exports ---

  ;; Called by the test to register the entity handle.
  (func (export "init_entity") (param $handle i64)
    (global.set $entity_handle (local.get $handle))
  )

  ;; tick(dt: f32) — increments the counter on the registered entity by 1.
  (func (export "tick") (param $dt f32)
    (local $handle i64)
    (local $val i64)
    (local.set $handle (global.get $entity_handle))
    ;; Skip if no entity registered yet.
    (if (i64.lt_s (local.get $handle) (i64.const 0)) (then return))
    ;; Read current counter value.
    (local.set $val (call $get_counter (local.get $handle)))
    ;; Increment by 1 and write back.
    (drop (call $set_counter (local.get $handle) (i64.add (local.get $val) (i64.const 1))))
  )
)
