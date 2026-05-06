;; W04 cap-gate fixture — declares NO effects in its manifest but
;; tries to import `wasi:sockets/tcp.connect`. Because the engine
;; only links the network shim when `<network>` is in the plugin's
;; declared effect set, the linker must fail at instantiate-time
;; with `EngineError::LinkerMissing`.
;;
;; The rcad-effects manifest appended by the test harness is
;; deliberately empty, so the engine binds NO host functions in the
;; `wasi:sockets/tcp` namespace — leaving this import unresolved.

(module
  (import "wasi:sockets/tcp" "connect" (func $connect (param i32 i32) (result i32)))

  (memory (export "memory") 1)

  (func $tick (export "tick") (param $dt f32)
    (drop (call $connect (i32.const 0) (i32.const 80)))
  )
)
