//! [`EcsBridge`] — wasmtime host functions for the ECS bridge.
//!
//! # Prototype scope ("very small initially")
//!
//! The component bridge is hard-coded for [`Counter`] (`i64` value). A generic
//! type-erased component bridge requires archetype iteration with type-erased
//! column access — not yet available in `kernel/ecs`'s single-archetype layout.
//! The generic bridge is a **Phase 4-Foundation** extension.
//!
//! # Wasm import names (stable)
//!
//! | Host function     | Wasm import                             |
//! |-------------------|-----------------------------------------|
//! | `entity_count`    | `rge.ecs::entity_count() -> i64`        |
//! | `spawn`           | `rge.ecs::spawn() -> i64`               |
//! | `despawn`         | `rge.ecs::despawn(i64) -> i32`          |
//! | `advance_tick`    | `rge.ecs::advance_tick()`               |
//! | `get_counter`     | `rge.ecs::get_counter(i64) -> i64`      |
//! | `set_counter`     | `rge.ecs::set_counter(i64, i64) -> i32` |
//! | `diagnostic_emit` | `rge.diagnostic::emit(i32, i32, i32)`   |

use rge_kernel_diagnostics::Severity;
use rge_kernel_ecs::{Component, EntityId, World};
use wasmtime::{Caller, Linker};

use crate::host_state::HostState;

// ---------------------------------------------------------------------------
// Counter — the prototype component
// ---------------------------------------------------------------------------

/// Prototype ECS component: a single signed 64-bit counter.
///
/// This is the only component type exposed through the Phase 3.2 ECS bridge.
/// The generic component bridge (WIT-typed, reflection-driven) is deferred to
/// Phase 4-Foundation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counter {
    /// The current counter value.
    pub value: i64,
}

impl Component for Counter {}

// ---------------------------------------------------------------------------
// EntityId <-> i64 encoding
// ---------------------------------------------------------------------------

/// Encode an [`EntityId`] as a non-negative `i64` for WASM ABI.
///
/// Uses the low 63 bits of the ULID (128-bit) — the sign bit is forced to 0
/// so the wasm side can use signed comparisons (e.g., `< 0` as an
/// "uninitialized" sentinel) without the host accidentally producing a
/// negative handle that aliases the sentinel space. Collisions are
/// astronomically unlikely in prototype use; a proper 128-bit handle
/// encoding is Phase 4 work.
#[must_use]
pub fn entity_id_to_i64(id: EntityId) -> i64 {
    // Mask to low 63 bits — guarantees the result is non-negative when
    // interpreted as i64. Loses one bit of entity-distinguishing entropy;
    // 2^63 distinct handles is still vastly more than any conceivable scene.
    #[allow(clippy::cast_possible_wrap)]
    {
        (id.ulid().0 & 0x7fff_ffff_ffff_ffff_u128) as i64
    }
}

/// Sentinel returned by `get_counter` when the entity handle is not found.
pub(crate) const ENTITY_NOT_FOUND: i64 = i64::MAX;

/// Find an entity by its truncated i64 handle using `World::query`.
///
/// O(n) over Counter-bearing entities — acceptable for prototype workloads.
/// Phase 4: `World` gains an `entity_ids()` iterator for handle-less traversal.
fn find_entity_by_handle(world: &World, handle: i64) -> Option<EntityId> {
    world.query::<Counter>().find_map(|(id, _)| {
        if entity_id_to_i64(id) == handle {
            Some(id)
        } else {
            None
        }
    })
}

// ---------------------------------------------------------------------------
// EcsBridge
// ---------------------------------------------------------------------------

/// Wires ECS host functions onto a [`wasmtime::Linker<HostState>`].
///
/// Call [`EcsBridge::install`] before instantiating a module. The installed
/// functions use stable wasm import names; only additions are permitted
/// between Phase 3 and Phase 4.
pub struct EcsBridge;

impl EcsBridge {
    /// Register all ECS + diagnostic bridge functions with `linker`.
    ///
    /// # Errors
    ///
    /// Returns a [`wasmtime::Error`] if any function name conflicts with an
    /// already-registered import.
    pub fn install(linker: &mut Linker<HostState>) -> Result<(), wasmtime::Error> {
        // ------------------------------------------------------------------
        // rge.ecs::entity_count() -> i64
        // ------------------------------------------------------------------
        linker.func_wrap(
            "rge.ecs",
            "entity_count",
            |mut caller: Caller<'_, HostState>| -> i64 {
                let world = caller.data_mut().world();
                // entity_count() returns usize; the cast is intentional and
                // prototype-safe (no world will have > i64::MAX entities).
                #[allow(clippy::cast_possible_wrap)]
                {
                    world.entity_count() as i64
                }
            },
        )?;

        // ------------------------------------------------------------------
        // rge.ecs::spawn() -> i64
        // Spawns a new entity (no components) and returns its handle.
        // ------------------------------------------------------------------
        linker.func_wrap(
            "rge.ecs",
            "spawn",
            |mut caller: Caller<'_, HostState>| -> i64 {
                let world = caller.data_mut().world();
                let id = world.spawn();
                entity_id_to_i64(id)
            },
        )?;

        // ------------------------------------------------------------------
        // rge.ecs::despawn(id: i64) -> i32
        // Returns 1 if the entity was found and despawned, 0 otherwise.
        // Note: searches only Counter-bearing entities (prototype scope).
        // ------------------------------------------------------------------
        linker.func_wrap(
            "rge.ecs",
            "despawn",
            |mut caller: Caller<'_, HostState>, handle: i64| -> i32 {
                let world = caller.data_mut().world();
                if let Some(id) = find_entity_by_handle(world, handle) {
                    if world.despawn(id) {
                        return 1;
                    }
                }
                0
            },
        )?;

        // ------------------------------------------------------------------
        // rge.ecs::advance_tick()
        // ------------------------------------------------------------------
        linker.func_wrap(
            "rge.ecs",
            "advance_tick",
            |mut caller: Caller<'_, HostState>| {
                caller.data_mut().world().advance_tick();
            },
        )?;

        // ------------------------------------------------------------------
        // rge.ecs::get_counter(id: i64) -> i64
        // Returns the Counter value, 0 if no Counter, MAX if entity missing.
        // ------------------------------------------------------------------
        linker.func_wrap(
            "rge.ecs",
            "get_counter",
            |mut caller: Caller<'_, HostState>, handle: i64| -> i64 {
                let world = caller.data_mut().world();
                for (id, counter) in world.query::<Counter>() {
                    if entity_id_to_i64(id) == handle {
                        return counter.value;
                    }
                }
                ENTITY_NOT_FOUND
            },
        )?;

        // ------------------------------------------------------------------
        // rge.ecs::set_counter(id: i64, value: i64) -> i32
        // Returns 1 on success, 0 if entity handle not found.
        // Collect matching IDs first to avoid query-then-mutate borrow conflict.
        // ------------------------------------------------------------------
        linker.func_wrap(
            "rge.ecs",
            "set_counter",
            |mut caller: Caller<'_, HostState>, handle: i64, value: i64| -> i32 {
                let world = caller.data_mut().world();
                let found: Option<EntityId> = world.query::<Counter>().find_map(|(id, _)| {
                    if entity_id_to_i64(id) == handle {
                        Some(id)
                    } else {
                        None
                    }
                });
                if let Some(id) = found {
                    world.insert(id, Counter { value });
                    1
                } else {
                    0
                }
            },
        )?;

        // ------------------------------------------------------------------
        // rge.diagnostic::emit(severity: i32, msg_ptr: i32, msg_len: i32)
        // Reads UTF-8 from wasm linear memory at (ptr, len).
        // severity: 0=suggestion 1=info 2=warning 3+=error
        // ------------------------------------------------------------------
        linker.func_wrap(
            "rge.diagnostic",
            "emit",
            |mut caller: Caller<'_, HostState>, severity: i32, ptr: i32, len: i32| {
                let mem = caller
                    .get_export("memory")
                    .and_then(wasmtime::Extern::into_memory);
                let message = if let Some(mem) = mem {
                    // ptr/len are WASM i32 ABI; reinterpret as unsigned offsets.
                    // Negative values saturate to 0 via checked arithmetic.
                    let offset = usize::try_from(ptr).unwrap_or(0);
                    let length = usize::try_from(len).unwrap_or(0);
                    let data = mem.data(&caller);
                    if offset + length <= data.len() {
                        String::from_utf8_lossy(&data[offset..offset + length]).into_owned()
                    } else {
                        "(script: diagnostic ptr out of bounds)".to_owned()
                    }
                } else {
                    "(script: no linear memory for diagnostic)".to_owned()
                };

                let sev = match severity {
                    0 => Severity::Suggestion,
                    1 => Severity::Info,
                    2 => Severity::Warning,
                    _ => Severity::Error,
                };
                caller.data_mut().emit_severity(sev, message);
            },
        )?;

        Ok(())
    }
}
