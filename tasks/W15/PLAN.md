# Wave W15 — pak-format

> Self-contained agent dispatch. Phase 4 deliverable per IMPLEMENTATION.md.
> Cross-refs: PLAN.md §1.6.3 (cooked binary), §1.6.10 (determinism guarantees).

## Goal

`.rge-pak` binary writer + reader. Header, sorted asset index, zstd compression, deterministic byte-identical output.

## Crate owned

`crates/pak-format`.

## Files this wave touches

```
crates/pak-format/src/{lib.rs, header.rs, index.rs, writer.rs, reader.rs, signature.rs}
crates/pak-format/tests/{round_trip.rs, determinism_gate.rs, mmap_test.rs}
```

## Stubs needed

- `zstd` workspace dep.
- `blake3` for asset content hashing.
- `ed25519-dalek` for signature stub (Phase 5 marketplace integrity — stub at v0.0.1).

## Implementation order

1. `header.rs` — magic `RGEP` (4 bytes), engine version (4 bytes), player-state schema version (4 bytes), flags (4 bytes), compression algo enum (1 byte), reserved (15 bytes) = 32 bytes total.
2. `index.rs` — sorted asset index: `IndexEntry { asset_id: AssetId, offset: u64, length: u64, kind: AssetKind }`. O(log n) binary search lookup.
3. `writer.rs` — `PakWriter::new() / .add_asset(id, kind, bytes) / .finish() -> Vec<u8>`. zstd-compresses each blob; sorts asset_id deterministically; writes header → index → blobs.
4. `reader.rs` — `PakReader::open(path) -> Pak` (mmap'd); `pak.lookup(asset_id) -> Option<&[u8]>` (decompresses on access).
5. `signature.rs` — Ed25519 stub. Sign over header+index hash. Phase 5 makes this real.
6. Test: write 100 dummy assets to `.rge-pak`; read back byte-identical.
7. Test: index lookup is O(log n) (verify with 10k entries).
8. **Determinism gate test**: two writes of identical assets in identical order → byte-identical pak (CI gate per §13.4).

## Rustforge prior art (steal-and-adapt)

| Source | Relevance | Adaptation |
|---|---|---|
| `rustforge/crates/persistence/` | content store patterns | reference for content-addressing |
| `rustforge/crates/manufacturing-format-ctb/` | binary format with versioned header | adapt header pattern |
| `rustforge/crates/manufacturing-format-cws/` | binary format reference | adapt index pattern |

Header pattern: `// adapted from rustforge::crates::manufacturing-format-ctb on 2026-05-05 — header+index pattern for .rge-pak`.

## Exit criteria

- Write 100 dummy assets to `.rge-pak`; read back byte-identical.
- Index lookup O(log n) on 10k entries.
- **Determinism gate**: two cooks of same source → byte-identical pak (verify with `diff --binary`).
- 100MB pak loads in <500ms (mmap + decompress on demand).
- `cargo test -p rge-pak-format` passes.

## Duration estimate

2 days.

## Anti-pattern check

PASS — single binary format (`.rge-pak`). No per-platform forks; one format for all targets (per-tier variants live within blobs).

## Handoff

After merge: W16 asset-store stores cooked blobs in `.rge-pak`; W17 io-gltf cook output goes to pak; build-pipeline (post-W15) orchestrates pak generation per cook target.
