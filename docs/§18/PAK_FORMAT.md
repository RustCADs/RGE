# PAK_FORMAT

| Companion to | PLAN.md §1.6 (file format discipline) + §1.6.4 (cooked-binary container) + §1.6.10 (cook determinism) + §13.4 (CI cook gate); IMPLEMENTATION.md Phase 4.2 (`crates/pak-format` shipped W15) |
|---|---|
| Status | Stable v1; 42 tests (41 active + 1 hardware-gated `#[ignore]`); shipped W15 per Status.md 2026-05-09; consumed by `crates/asset-store` for the cooked-pak load path; v0.0.1 wire format frozen with reserved Phase-5 marketplace signature region |
| Audience | Authors writing or reading `.rge-pak` containers; the asset-store integrating cooked output; reviewers verifying the byte-identical-cook gate; future Phase-5 marketplace authors filling the signature stub |
| Sibling doc | `KERNEL_ASSET.md` — canonical `AssetId` owner pak-format re-exports; `RGE_DATA.md` — schemas pak-format serialises as `AssetKind::{Scene, Prefab, Project}` payloads |
| Reference impls | `crates/pak-format/src/lib.rs` (121L) · `crates/pak-format/src/header.rs` (245L; 7 unit tests) · `crates/pak-format/src/index.rs` (431L; 9 unit tests) · `crates/pak-format/src/writer.rs` (316L; 5 unit tests) · `crates/pak-format/src/reader.rs` (311L; 5 unit tests) · `crates/pak-format/src/signature.rs` (89L; 3 unit tests) · `crates/pak-format/src/errors.rs` (86L; 0 unit tests) · 4 integration test files at `crates/pak-format/tests/` (13 tests; 1 `#[ignore]`) |

> Convention defined by `PLUGIN_HOST_PATTERNS.md` §header. This doc is the workspace-wide reference for the `.rge-pak` cooked-asset container substrate. Source schemas (`Project` / `Scene` / `Prefab`) that pak-format serialises as payload bytes live in `crates/rge-data` and are covered by `RGE_DATA.md`; the canonical `AssetId` re-exported by pak-format lives in `kernel/asset` and is covered by `KERNEL_ASSET.md`.

## 1. The cook-determinism contract

PLAN.md §1.6.10 commits to **byte-identical cook**: two writes of identical assets in identical order produce a byte-identical `.rge-pak`. PLAN.md §13.4 promotes this property to a CI-blocking gate. The substrate is the foundation that asset-store (cooked-content cache), marketplace signing, and content-addressed delta updates all build on — without bit-determinism, none of those layers can use the pak's bytes as a stable hash input.

The five mechanical guarantees (per `writer.rs` lines 1-22 module-doc):

1. **Sort by `AssetId` before serialisation.** `PakWriter::finish` calls `sort_by(|a, b| a.0.cmp(&b.0))` on the staged-asset list (lexicographic on raw 32-byte BLAKE3 digest — architecture-endianness-independent). Stable sort preserves input order for ties, then `dedup_keep_last_by_id` collapses duplicates last-wins.
2. **zstd at fixed `ZSTD_LEVEL = 3`, single-threaded.** zstd is bit-deterministic for a given (library version, level, input) on a single thread; `zstd::bulk::compress` is the chosen API. Changing the level changes the wire bytes — the constant carries the determinism contract and bumping it requires bumping `header::ENGINE_VERSION`.
3. **Reserved bytes zero-filled.** The 15-byte `RESERVED_LEN` block in the header is `w.write_all(&[0u8; RESERVED_LEN])?;` on every write — never `MaybeUninit` or read-back garbage.
4. **No timestamps, PIDs, random IVs, or multithreaded interleaving.** None of these appear in the writer's body; the only varying inputs are the staged asset payloads themselves.
5. **Signature region zero-filled when no signing key is supplied.** The trailing 64-byte Ed25519 region (`SIGNATURE_SIZE`) gets `out.extend_from_slice(&[0u8; SIGNATURE_SIZE]);` for unsigned paks. Two unsigned cooks of identical sources are byte-identical because zeros don't depend on any unspecified state.

Verified by `tests/determinism_gate.rs` (5 tests): same-source cook idempotent (`two_cooks_of_same_source_are_byte_identical`), input-order-permutation invariance (`determinism_holds_across_input_order_permutation`), uncompressed-codec determinism, signature-region zero-fill, duplicate-id collapse determinism.

## 2. Format overview — four contiguous regions

Per `lib.rs` lines 5-17:

```text
+-----------------------------+
| Header (32 bytes, fixed)    |  magic "RGEP" + versions + flags
+-----------------------------+
| Index region                |  u64 entry_count + entry_count * IndexEntry
+-----------------------------+
| Blob arena                  |  zstd-compressed blobs, contiguous
+-----------------------------+
| Optional Ed25519 signature  |  Phase-5 marketplace integrity (stub at v0.0.1)
+-----------------------------+
```

The five module split mirrors the four regions plus an error type:

- **`header`** — fixed 32-byte top-of-file structure (`magic` + `engine_version` + `player_state_schema_version` + `flags` + `compression_algo` + 15 reserved zero bytes).
- **`index`** — sorted `IndexEntry` list with binary-search lookup and `read_from`-time sortedness validation.
- **`writer`** — `PakWriter` builder: stage assets, sort, compress, lay out, emit.
- **`reader`** — `PakReader`: open via `path` (mmap) or `bytes` (owned), O(log n) lookup, lazy decompression.
- **`signature`** — Phase-5 marketplace stub; reserves 64 trailing bytes for Ed25519, currently zero-filled.

`errors` flattens every failure path into a single `PakError` enum (§5).

## 3. `PakWriter` — deterministic builder

Lives at `crates/pak-format/src/writer.rs`. Stages assets, then `finish()` returns the full pak bytes:

```rust
pub struct PakWriter { /* private */ }

impl PakWriter {
    pub fn new() -> Self;                                                     // default: zstd
    pub fn with_compression(algo: CompressionAlgo) -> Self;                   // None | Zstd
    pub fn add_asset(&mut self, asset_id: AssetId, kind: AssetKind, bytes: Vec<u8>);
    pub fn add_asset_auto_id(&mut self, kind: AssetKind, bytes: Vec<u8>) -> AssetId;
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
    pub fn finish(self) -> Result<Vec<u8>, PakError>;
}
```

`add_asset` takes an explicit id — used when the cook pipeline already computed it (asset-store's content-addressing). `add_asset_auto_id` computes `AssetId::from_bytes(&bytes)` and forwards. Duplicate ids are collapsed last-wins by both `dedup_keep_last_by_id` (in writer's compressed-blob list) and `IndexTable::from_unsorted` (in the index table built from those compressed blobs).

`finish` is a five-step pipeline (lines 142-225):

1. **Compress** every staged blob upfront (so each blob's compressed length is known).
2. **Sort** `compressed_blobs` by `AssetId` via stable `sort_by`. **This is the determinism point.**
3. **Compute layout offsets** — index region size depends on post-dedup entry count; dedup runs here.
4. **Build `IndexTable`** with absolute file offsets (`HEADER_SIZE + index_wire_size + cumulative_blob_offset`).
5. **Serialise** in fixed order: header → index → blobs → 64-byte zero-filled signature region.

The pre-allocated output `Vec<u8>` has its size fully known after the layout pass, so `finish` makes a single allocation. `debug_assert_eq!` after every region-write pins the layout-arithmetic invariants.

`ZSTD_LEVEL = 3` is the zstd default — a balance between cook time and decompression speed at runtime. Higher levels (e.g. 19) yield smaller paks but ~30× slower cooks; the cook budget is more constrained than the distribution-size budget.

## 4. `PakReader` — mmap-backed, lazy decompression

Lives at `crates/pak-format/src/reader.rs`. Opens via mmap (production) or owned bytes (tests / network-fetched / `include_bytes!` consumers):

```rust
pub struct PakReader { /* private */ }
pub type PakBlob<'a> = Cow<'a, [u8]>;

impl PakReader {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, PakError>;     // mmap
    pub fn open_bytes(bytes: Vec<u8>) -> Result<Self, PakError>;        // owned
    pub fn header(&self) -> &PakHeader;
    pub fn index(&self) -> &IndexTable;
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
    pub fn lookup(&self, id: &AssetId) -> Result<Option<PakBlob<'_>>, PakError>;
    pub fn iter(&self) -> impl Iterator<Item = (&IndexEntry, Result<PakBlob<'_>, PakError>)>;
    pub fn first_of_kind(&self, kind: AssetKind) -> Option<(&IndexEntry, Result<PakBlob<'_>, PakError>)>;
}
```

The two open paths converge on a single `from_backing` parser that:

1. Validates `bytes.len() >= HEADER_SIZE` (else `PakError::Truncated`).
2. Parses the fixed 32-byte header.
3. Parses the index region; `IndexTable::read_from` validates the sorted-strictly-ascending invariant.
4. **Eagerly validates every blob extent.** `entry.offset + entry.compressed_length <= file_size` is checked at open time (O(n) over the index — already-paid by the index parse). This is the primary defence against a maliciously crafted pak that points blobs out of bounds.

`lookup` is O(log n) via `Vec::binary_search_by`. Decompression is lazy — only the requested blob is decompressed via `zstd::stream::read::Decoder`, pre-sized using the index's `uncompressed_length` hint. The returned `PakBlob<'_>` is `Cow::Borrowed` (zero-copy) on the uncompressed-codec path and `Cow::Owned` on the zstd path.

### mmap support — the only `unsafe` in the crate

The crate-level `unsafe_code = "deny"` (relaxed from the workspace `forbid` per `Cargo.toml` lines 49-56) accepts a single `#[allow(unsafe_code)]` site at `reader.rs` line 90: `unsafe { Mmap::map(&file)? }`. The SAFETY proof (lines 82-89):

> "`Mmap::map` is `unsafe` because UB arises only if another process truncates or writes to the underlying file while we hold the map. For a read-only cooked pak (which the asset-store treats as immutable once written) the same risk window exists for ordinary `File::read` — we accept it explicitly. This is the only `unsafe` in the crate; the workspace `unsafe_code = forbid` lint is locally relaxed to `deny` in this crate's `Cargo.toml`."

The two open paths produce equivalent `PakReader` state (regression-pinned by `mmap_test.rs::open_via_path_and_via_bytes_yield_same_index`); choosing mmap vs owned bytes is a perf decision (mmap is O(1) in file size on Win/macOS/Linux), not a correctness one.

## 5. Public API surface — types

Re-exported from `lib.rs` lines 113-120:

```rust
// errors
pub use errors::PakError;

// header — wire constants + parsed struct
pub use header::{
    CompressionAlgo, PakHeader, ENGINE_VERSION, HEADER_SIZE, MAGIC,
    PLAYER_STATE_SCHEMA_VERSION,
};

// index — sorted entries with binary-search lookup
pub use index::{AssetId, AssetKind, IndexEntry, INDEX_ENTRY_SIZE};

// reader / writer
pub use reader::PakReader;
pub use writer::PakWriter;

// signature stub
pub use signature::{verify_signature, SIGNATURE_SIZE};
```

### `PakError` taxonomy

```rust
pub enum PakError {
    BadMagic { got: [u8; 4] },
    UnknownCompressionAlgo(u8),
    Truncated { got: usize, need: usize },
    IndexOutOfBounds { entry_count: u64, entry_size: usize, needed: usize, available: usize },
    BlobOutOfBounds { asset_id: String, offset: u64, length: u64, file_size: u64 },
    IndexNotSorted { index: usize },
    Zstd(String),
    Io(#[from] std::io::Error),
}
```

Variants distinguish **structural** failures (bad magic, truncated region, index OOB, blob OOB, sort-invariant violation) from **codec** failures (zstd, IO). The reader and writer share this type. `IndexNotSorted { index }` carries the position where the violation was observed (1-based). `BlobOutOfBounds` formats `asset_id` as the user-facing `"blake3:<hex>"` string for actionable error messages.

### `AssetKind` enum (wire u16, stable values)

```rust
pub enum AssetKind {
    Opaque   = 0,    // generic blob / test fixture
    Mesh     = 1,    // triangle mesh
    Texture  = 2,    // 2D texture
    Audio    = 3,    // PCM audio
    Material = 4,    // PBR
    AnimClip = 5,    // animation clip
    Shader   = 6,    // pre-compiled WGSL/SPIR-V/DXIL
    Script   = 7,    // AOT-compiled WASM (`.cwasm`)
    Scene    = 8,    // scene graph dump
    Prefab   = 9,    // prefab template
    Unknown  = 0xFFFF,
}
```

Stable enum values across engine versions; appending a new kind is a non-breaking change (existing readers see the new value as `Unknown`). **Reordering existing values would invalidate every existing pak** — the values are part of the wire format. `from_u16` maps unknown wire values to `Unknown` for forward-compat (a newer cooker may have introduced a kind this reader doesn't know).

### `IndexEntry` — 60 bytes per row

```text
offset  size  field
0x00    32    asset_id (raw 32-byte BLAKE3 digest)
0x20    8     offset (u64 LE) — absolute file offset of blob
0x28    8     compressed_length (u64 LE)
0x30    8     uncompressed_length (u64 LE) — pre-sizes decompression buffer
0x38    2     kind (u16 LE — AssetKind enum)
0x3A    2     flags (u16 LE — currently zero; future per-blob bits)
total:  60 bytes  (= INDEX_ENTRY_SIZE)
```

The raw 32-byte digest is stored on the wire (no `"blake3:"` prefix); the prefix is reconstructed by `AssetId::to_string()`. Saves 8 bytes per entry × 10k entries = 80 KB of header bloat for a 10k-asset pak.

### `PakHeader` — 32 bytes fixed

```text
offset  size  field
0x00    4     magic = b"RGEP"           (literal ASCII; visible in hex-dump)
0x04    4     engine_version (u32 LE)   = 1
0x08    4     player_state_schema_version (u32 LE) = 1   (independent bump per §1.6.9)
0x0C    4     flags (u32 LE)            = 0  (bitfield reserved)
0x10    1     compression_algo (u8 enum) = 0 (None) | 1 (Zstd)
0x11    15    reserved (zero)
total:  32 bytes  (= HEADER_SIZE)
```

`MAGIC = *b"RGEP"` is stored as a literal byte sequence (NOT a `u32`) so a hex-dump of the file shows it in cleartext at offset 0. `ENGINE_VERSION = 1` and `PLAYER_STATE_SCHEMA_VERSION = 1` for v1; an algo-byte change requires `ENGINE_VERSION` bump (per §1).

`HEADER_SIZE` is a wire constant, NOT `size_of::<PakHeader>()` — the struct uses native Rust layout for in-memory ergonomics; serialisation is hand-rolled via `byteorder`.

## 6. Index — sorted, O(log n) lookup, validated on read

`IndexTable` (`crates/pak-format/src/index.rs` lines 191-303) maintains the strictly-ascending sortedness invariant on construction:

- **Writer side** (`from_unsorted`): stable `sort_by` then reverse-dedup-reverse for last-wins on duplicate ids.
- **Reader side** (`read_from`): bounds-check `entry_count <= MAX_INDEX_ENTRIES` (= 64 MiB / 60 = ~1.1 M entries — defensive cap against header-driven OOM at parse), read entries, then `validate_sorted` rejects any equal-or-decreasing pair with `PakError::IndexNotSorted { index }`.

Lookup is `Vec::binary_search_by(|e| e.asset_id.cmp(id))`. The 10k-entry round-trip test (`tests/round_trip.rs::ten_thousand_entries_lookup_correctness`) exercises 10 000 binary-search hits + a synthesized-id miss to pin correctness; the perf curve is structural (Rust stdlib's binary search is the substrate).

`wire_size()` = `8 (entry_count u64) + N * INDEX_ENTRY_SIZE` — used by the writer's layout-offset arithmetic.

## 7. Test coverage breakdown — 42 tests (41 active + 1 `#[ignore]`)

### Unit tests in `src/` (29 total)

- `header.rs` (7): `header_size_constant_is_thirty_two`, `header_round_trips_byte_identical`, `magic_appears_at_offset_zero_in_ascii`, `reserved_bytes_are_zero_after_write`, `bad_magic_is_detected`, `unknown_compression_algo_is_detected`, `two_writes_of_same_header_are_byte_identical`.
- `index.rs` (9): `entry_size_constant_is_sixty`, `entry_round_trips`, `from_unsorted_sorts_ascending`, `duplicate_ids_collapse_last_wins`, `lookup_is_log_n`, `validate_sorted_catches_unsorted`, `validate_sorted_catches_duplicates`, `assetid_display_is_blake3_hex`, `table_round_trips`.
- `writer.rs` (5): `empty_writer_emits_header_and_signature_only`, `finish_then_reader_round_trips_blob`, `sort_is_deterministic`, `duplicate_ids_collapse_last_wins`, `uncompressed_codec_round_trips`.
- `reader.rs` (5): `open_truncated_file_errors`, `open_bad_magic_errors`, `header_and_index_parse_after_writer`, `iter_yields_entries_in_sorted_order`, `lookup_miss_returns_none`.
- `signature.rs` (3): `signature_size_is_sixty_four`, `verify_zero_signature_is_ok`, `verify_nonzero_signature_is_ok_at_v0`.

### Integration tests in `tests/` (13 total; 1 `#[ignore]`)

- `determinism_gate.rs` (5): `two_cooks_of_same_source_are_byte_identical`, `determinism_holds_across_input_order_permutation`, `determinism_holds_for_uncompressed_codec_too`, `signature_region_is_zero_for_unsigned_paks`, `duplicate_id_collapse_is_deterministic`.
- `mmap_test.rs` (4): `mmap_open_round_trips`, `mmap_open_is_fast_for_multi_mb_pak` (asserts <100ms for 4 MB; the §13.4 100MB-load gate is hardware-dependent and gated separately), `lazy_decompression_per_lookup`, `open_via_path_and_via_bytes_yield_same_index`.
- `perf_smoke.rs` (1, `#[ignore]`): `perf_load_100mb_under_500ms` — the §13.4 CI exit-criterion check; marked `#[ignore]` so default CI doesn't pay the ~10s cook cost. Run via `cargo test -p rge-pak-format --release -- --ignored perf_load_100mb_under_500ms --nocapture`.
- `round_trip.rs` (3): `hundred_dummy_assets_round_trip_byte_identical`, `ten_thousand_entries_lookup_correctness`, `index_is_sorted_after_random_input`.

Together the 42 tests cover the byte-identical-cook gate, the strictly-ascending index invariant (writer + reader sides), the lazy-decompression contract, the mmap-vs-owned-bytes equivalence, and the malicious-pak defences (bad magic / truncated file / blob OOB / index sortedness).

## 8. AssetId integration — `pub use rge_kernel_asset::AssetId`

Per `index.rs` line 61:

```rust
pub use rge_kernel_asset::AssetId;
```

Post-2026-05-06 reconciliation, `kernel/asset` is the single canonical owner of `AssetId` per Status.md "AssetId canonical owner reconciliation" (cleared 2026-05-06; one of 7 duplicate `pub struct AssetId` definitions migrated to `pub use`). pak-format is one of the seven downstream crates that swung over.

The reconciled API used by pak-format:

- `AssetId::from_bytes(&[u8]) -> Self` — canonical content-addressing constructor (BLAKE3 hash).
- `AssetId::from_raw([u8; 32]) -> Self` — bypass for the wire-decoded raw digest case.
- `AssetId::raw() -> &[u8; 32]` — borrow underlying digest, used by `IndexEntry::write_into` to emit the 32-byte wire form.
- `AssetId::to_string() -> String` — `"blake3:<64-hex-lowercase>"` for `BlobOutOfBounds` error messages and human-readable diagnostics.
- `AssetId::cmp(...)` — total order on raw bytes; the substrate of the writer's deterministic sort step.

The `AssetId` text-form regression test (`assetid_display_is_blake3_hex` in `index.rs`) pins the canonical `"blake3:af1349b9f5..."` prefix property against the empty-input BLAKE3 vector — same vector pinned by `kernel/asset/tests/asset_id_compat_with_asset_store.rs::known_vector_matches_asset_store_cross_machine_determinism`. Cross-ref `KERNEL_ASSET.md` §2 for the canonical owner story.

## 9. Phase-5 marketplace signature stub

The 64-byte trailing region (`SIGNATURE_SIZE = ed25519_dalek::Signature::BYTE_SIZE = 64`) is reserved at v1 of the wire format precisely so turning on signing in Phase-5 does NOT break every pre-Phase-5 pak. Per `signature.rs` lines 4-23:

- v0.0.1 keeps the region zero-filled for unsigned paks. Determinism gate preserved (zeros don't depend on unspecified state).
- Phase-5 plan: signature covers `header_bytes ++ index_region_bytes` (i.e. `HEADER_SIZE + 8 + N * INDEX_ENTRY_SIZE` bytes). Index entries reference blobs by content-hash + offset+length, so covering the index transitively covers all blobs without re-hashing 100 MB of blob bytes on verification.
- `verify_signature(&[u8; 64]) -> Result<(), PakError>` is currently a no-op stub; Phase-5 will (1) take a `&VerifyingKey` parameter, (2) compute `blake3(header_bytes ++ index_region_bytes)`, (3) call `verifying_key.verify(&hash, signature)`, (4) return `Err(PakError::SignatureMismatch)` on failure.

The compile-time assertion `const _: [(); 64] = [(); SIGNATURE_SIZE];` traps any future `ed25519_dalek::Signature::BYTE_SIZE` change so the wire constant stays load-bearing.

## 10. Failure class — recoverable

`crates/pak-format/src/lib.rs` does **not** currently carry a `//! Failure class: <kind>` declaration; the crate appears in `tools/architecture-lints/exemptions.toml` (line 247):

```toml
[[exemption]]
lint = "failure-class"
file = "crates/pak-format/Cargo.toml"
reason = "Phase 1.x rollout debt - declaration added when crate gets first real implementation per IMPLEMENTATION.md."
```

The exemption is rollout-debt per the audit-1 deal (`RECOVERY_MODEL.md` §6); pak-format **is** implemented (not stub), but the declaration was not added when W15 landed and is tracked for cleanup as part of the 58 remaining failure-class rollout-debt entries.

The crate's intended class is **recoverable**: `PakError` failures (bad magic, truncated, blob OOB, sort-invariant violation, codec error, IO error) are all caller-recoverable — the cook pipeline branches on the error variant and surfaces actionable diagnostics. Pak corruption does NOT escalate to snapshot-recoverable because the cook can simply re-run from source assets; PIE state is unaffected by a failed pak load.

## 11. Source / spec inconsistencies

- **Brief stated `Phase 4.2 — crates/pak-format | shipped W15 — 41 tests`**; source-truth via `grep -c '#\[test\]'`: 42 test attributes total (29 unit + 13 integration). Status.md line 25 explicitly says "41 tests" which is the *active-test* count after subtracting the 1 `#[ignore]`-marked `perf_smoke::perf_load_100mb_under_500ms` (Status.md line 42 confirms "2 ignored tests with explanatory `#[ignore = "..."]` markers — `pak-format::perf_smoke` and `kernel/app::main_loop_test::ten_thousand_frames_under_100ms`"). Doc reflects both numbers.
- **Brief stated "AssetId integration (pak-format uses `pub use rge_kernel_asset::AssetId;` post-2026-05-06 reconciliation)"**; source-truth confirmed at `index.rs` line 61. The reconciliation closure in Status.md "AssetId consumer migration" (line 27) confirms pak-format is one of the seven crates that migrated. The doc reflects this verbatim.
- **Brief stated "mmap support if any (read source — there were SAFETY proof comments mentioned in change.md early entries)"**; source-truth: mmap support is present, single `#[allow(unsafe_code)]` site at `reader.rs` line 90 with explicit SAFETY proof in lines 82-89, plus a crate-level relaxation from workspace `forbid` to local `deny` in `Cargo.toml` lines 49-56. The doc reflects all three signal layers (lib.rs preamble, the `unsafe` site, the Cargo.toml lint relaxation).
- **Brief stated the perf gate is "perf_smoke is `#[ignore]`-marked hardware-gated"**; source-truth confirmed at `tests/perf_smoke.rs` lines 14-15 (`#[ignore]`) and lines 22-23 (the canonical run incantation `cargo test -p rge-pak-format --release -- --ignored perf_load_100mb_under_500ms --nocapture`). The doc reflects this verbatim.
- **Brief assumed pak-format declares a `//! Failure class:` value**; source-truth: pak-format does NOT carry the declaration (rollout-debt exemption still in place). The doc surfaces this honestly under §10 rather than papering over with an inferred class.

## 12. References

- **PLAN.md §1.6** — file format discipline; the `.rge-pak` cooked-binary container is the §1.6.4 commitment.
- **PLAN.md §1.6.4** — cooked binary; reserves the optional Ed25519 signature region for Phase-5 marketplace integrity.
- **PLAN.md §1.6.10** — cook determinism; the byte-identical-cook contract this substrate implements.
- **PLAN.md §13.4** — CI cook gate; promotes byte-identical cook + 100MB-load-<500ms to CI-blocking properties.
- **IMPLEMENTATION.md Phase 4.2** — `crates/pak-format` shipped W15; this doc lifts the W15 dispatch package's wire spec into a §18 reference.
- **`KERNEL_ASSET.md`** — sibling §18 doc; canonical `AssetId` owner pak-format re-exports per the 2026-05-06 reconciliation.
- **`RGE_DATA.md`** — sibling §18 doc; the `Project` / `Scene` / `Prefab` schemas pak-format serialises as `AssetKind::{Project, Scene, Prefab}` payload bytes.
- **`crates/pak-format/src/lib.rs`** — module roots + wire format spec + determinism contract.
- **`crates/pak-format/src/header.rs`** — fixed 32-byte top-of-file header + `CompressionAlgo` enum + 7 unit tests.
- **`crates/pak-format/src/index.rs`** — `AssetKind` enum + `IndexEntry` 60-byte row + `IndexTable` sorted with binary-search lookup + 9 unit tests.
- **`crates/pak-format/src/writer.rs`** — `PakWriter` builder + `ZSTD_LEVEL = 3` constant + 5 unit tests.
- **`crates/pak-format/src/reader.rs`** — `PakReader` mmap-or-owned + lazy decompression + the single SAFETY-proven `unsafe` site + 5 unit tests.
- **`crates/pak-format/src/signature.rs`** — 64-byte Phase-5 signature stub + `verify_signature` + `SIGNATURE_SIZE` wire constant + 3 unit tests.
- **`crates/pak-format/src/errors.rs`** — `PakError` taxonomy.
- **`crates/pak-format/tests/determinism_gate.rs`** — the 5-test byte-identical-cook gate (CI-blocking).
- **`crates/pak-format/tests/mmap_test.rs`** — mmap-vs-owned-bytes equivalence + lazy-decompression contract (4 tests).
- **`crates/pak-format/tests/perf_smoke.rs`** — the §13.4 100MB-load-<500ms gate (1 test, `#[ignore]`-marked hardware-gated).
- **`crates/pak-format/tests/round_trip.rs`** — 100-asset round-trip + 10k-entry lookup correctness + random-input sortedness (3 tests).
- **`crates/asset-store/src/`** — Tier-2 consumer; cooked-pak load path uses `PakReader` + `AssetId` re-export.
