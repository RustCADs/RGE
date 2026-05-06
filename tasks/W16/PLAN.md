# Wave W16 — asset-store

> Self-contained agent dispatch. Phase 4 deliverable per IMPLEMENTATION.md.
> Cross-refs: PLAN.md §1.2.4 (zero-copy asset views), §1.6.3 (cooked binary).

## Goal

Content-addressed local cache. BLAKE3 keying. File-system layout. `Cache` trait that other crates stub against.

## Crate owned

`crates/asset-store`.

## Files this wave touches

```
crates/asset-store/src/{lib.rs, cache.rs, local.rs, asset_id.rs, layout.rs, dependency.rs}
crates/asset-store/tests/{dedup_test.rs, eviction_test.rs, layout_test.rs}
```

## Stubs needed

- `blake3` workspace dep.

## Implementation order

1. `asset_id.rs` — `AssetId(blake3:<hash>)`; `from_bytes(&[u8]) -> AssetId` content-hashes input.
2. `layout.rs` — `~/.cache/rge/assets/<2-char-prefix>/<full-hash>` directory layout (avoids 1M files in single dir).
3. `cache.rs` — `trait Cache { fn get(&self, id: AssetId) -> Option<Bytes>; fn put(&mut self, bytes: Bytes) -> AssetId; fn evict_lru(&mut self, max_bytes: u64); }`. Other crates stub against this trait.
4. `local.rs` — `LocalCache` impl: filesystem-backed.
5. `dependency.rs` — track dep edges: asset A depends on assets B, C. For invalidation cascade.
6. Test: insert same bytes twice → same AssetId, single storage entry (dedup verified).
7. Test: query by AssetId returns bytes byte-identical to insert.
8. Test: LRU eviction respects `max_bytes` cap.

## Rustforge prior art (steal-and-adapt)

| Source | Relevance | Adaptation |
|---|---|---|
| `rustforge/crates/persistence/` | content store / cache patterns | direct adapt; this is the closest precursor |
| `rustforge/crates/runtime-wasmtime/src/lib.rs` | uses `blake3` for plugin_id (content-addressable) | reference for content-addressing convention |

Header pattern: `// adapted from rustforge::crates::persistence on 2026-05-05 — content-addressed cache for general assets`.

## Exit criteria

- Insert same bytes twice → same AssetId, single storage entry.
- Query by AssetId returns bytes byte-identical to insert.
- LRU eviction works on `max_bytes` cap.
- Cross-machine determinism: same input bytes on different machines produce same AssetId.
- `cargo test -p rge-asset-store` passes.

## Duration estimate

2 days.

## Anti-pattern check

PASS — single content-addressed store. `Cache` trait abstracts implementation; LocalCache is one impl (could swap to remote/distributed later post-v1).

## Handoff

After merge: W17 io-gltf consumes Cache to store imported assets; W18 io-image same; W15 pak-format reads from Cache during cook; W14 rge-data uses AssetId in Scene component refs.
