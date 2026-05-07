# RGE_DATA

| Companion to | PLAN.md §1.6 (file format discipline) + §1.6.1 (RON as single source-format family) + §1.6.3 (identity scheme: ULID entities + content-addressed `AssetId`) + §1.6.6 (project layout) + §1.6.7 (versioning + migration); IMPLEMENTATION.md Phase 4.3 (`crates/rge-data` shipped W14) |
|---|---|
| Status | Stable v1; 81 tests (37 unit + 44 integration); shipped W14 per Status.md 2026-05-09; v0.1 schema is the current target; v0.0 → v0.1 migration is the only registered transformation today |
| Audience | Editor / asset-pipeline authors loading or saving `.rge-project` / `.rge-scene` / `.rge-prefab` files; reviewers verifying the round-trip-stability gate; future schema-bump authors registering a v0.1 → v0.2 migration |
| Sibling doc | `KERNEL_ASSET.md` — canonical `AssetId` owner rge-data re-exports; `PAK_FORMAT.md` — the `.rge-pak` cooked container that serialises rge-data schemas as `AssetKind::{Project, Scene, Prefab}` payloads |
| Reference impls | `crates/rge-data/src/lib.rs` (71L) · `crates/rge-data/src/schema_version.rs` (249L; 10 unit tests) · `crates/rge-data/src/entity_ref.rs` (~150L; 7 unit tests) · `crates/rge-data/src/asset_ref.rs` (~120L; 10 unit tests) · `crates/rge-data/src/project.rs` (~200L; 3 unit tests) · `crates/rge-data/src/scene.rs` (~250L; 3 unit tests) · `crates/rge-data/src/prefab.rs` (~150L; 2 unit tests) · `crates/rge-data/src/migration.rs` (557L; 10 unit tests) · 3 integration test files at `crates/rge-data/tests/` (36 tests) |

> Convention defined by `PLUGIN_HOST_PATTERNS.md` §header. This doc is the workspace-wide reference for the `Project` / `Scene` / `Prefab` source-file schemas and the migration substrate that brings older payloads forward. The `.rge-pak` cooked container that serialises these schemas as opaque payload bytes lives in `crates/pak-format` and is covered by `PAK_FORMAT.md`.

## 1. Why a substrate

Every authored game project is a tree of three file kinds: one `.rge-project` at the root, N `.rge-scene` files for authored scenes, and M `.rge-prefab` files for reusable entity bundles. PLAN §1.6.1 commits to **RON as the single source-format family** (no JSON, no YAML, no binary alternative at this layer); PLAN §1.6.6 fixes the project-layout subtree shape (`assets/`, `scenes/`, `prefabs/`, `materials/`, `scripts/`, `plugins/`, `target/cook/`); PLAN §1.6.7 commits to per-file `version: "x.y.z"` schema headers driving migration on load.

`rge-data` is the canonical home for the three schema struct types + the migration registry + the SchemaVersion / EntityId / AssetId identity primitives:

- **Single source of truth for the wire shape.** Project, Scene, Prefab are `Serialize + Deserialize`-derived; RON is the wire format; round-trip RON → struct → RON is byte-identical given the canonical pretty-print config.
- **Schema-versioned with the migration registry.** Every source file opens with `version: "0.1.0"`; the loader walks `MigrationRegistry::chain` to bring older payloads forward before deserialisation succeeds.
- **Reflection-neutral envelopes.** `ComponentValue { type_id, data }` carries components as `(canonical type path, RON literal)` pairs — the crate has **zero** dependency on `kernel/types::Reflect` (whose lifetime / cadence is per-Reflect-type, not per-file-format). When W02 reflection lands, callers parse the scene first, then walk each `data` string through reflection-aware bridging.

## 2. `SchemaVersion` — the dotted-triple version header

Lives at `crates/rge-data/src/schema_version.rs`. The `version: "x.y.z"` field at the top of every file:

```rust
pub struct SchemaVersion {
    pub major: u8,    // breaking — new migration entry required
    pub minor: u8,    // additive, default-compatible
    pub patch: u8,    // pure renames / doc-only
}

impl SchemaVersion {
    pub const fn new(major: u8, minor: u8, patch: u8) -> Self;
    pub const V0_0_0: Self = Self::new(0, 0, 0);
    pub const V0_1_0: Self = Self::new(0, 1, 0);
    pub const fn is_compatible_with(self, other: Self) -> bool;   // same-major check
}
```

### Wire form: quoted dotted triple

Custom `Serialize` / `Deserialize` impls emit/parse the version as a **single quoted string** rather than a struct of three `u8`s. Per `schema_version.rs` lines 16-32 source RON looks like:

```ron
Project (
    version: "0.1.0",
    ...
)
```

…rather than `version: SchemaVersion(major: 0, minor: 1, patch: 0)`. The former is what humans expect; the latter would need round-trip discipline against serde's struct shape.

### `u8` components — bounds and cadence

Per the W14 dispatch package, each component is `u8`. RGE's release cadence (yearly major bumps at most) means 256 majors is a hundred years of releases; small types keep the AST representation tight and serde RON output compact. Cross-ref with `kernel/types::SchemaVersion` (W02): that uses `u16` and is geared toward per-Reflect-type version tags; this crate's `SchemaVersion` is the file-format header version — different lifetime / cadence, deliberately distinct.

### `SchemaVersionParseError`

Three granular variants (so a corrupted file surfaces *which* part is malformed):

```rust
pub enum SchemaVersionParseError {
    MalformedShape(String),         // not "major.minor.patch"
    ComponentOutOfRange(String),    // component exceeds u8::MAX
    NonNumeric(String),             // component not numeric
}
```

`Ord` is lexicographic (major dominates, then minor, then patch). 10 unit tests pin: ordering, major-compat check, dotted-triple display, basic + edge-case parses, RON quoted-string round-trip, default = `V0_0_0`.

## 3. `EntityId` — scene-stable ULID

Lives at `crates/rge-data/src/entity_ref.rs`. The per-entity identifier inside `.rge-scene` / `.rge-prefab`:

```rust
pub struct EntityId(pub Ulid);

impl EntityId {
    pub const NIL: Self = Self(Ulid(0));
    pub const fn from_u128(v: u128) -> Self;
    pub const fn to_u128(self) -> u128;
    pub const fn as_ulid(&self) -> &Ulid;
    pub fn to_canonical(self) -> String;     // 26-char Crockford-base32
}
```

### Display (lossy) vs serde (full)

- **On disk (RON):** the full 26-character ULID string (e.g. `"01H...XYZ"`), round-trip stable. `#[serde(transparent)]` so the on-disk RON is a bare quoted string, not `EntityId("...")`.
- **In `Display`:** an 8-hex-character prefix prefixed by `e_` (e.g. `e_abc12345`). This is **lossy** — never re-parse a `Display` string back into an `EntityId`. Used for inspector labels and diagnostic spans only. Per PLAN §1.6.3.

### Why ULID (not bare u64)

Per `entity_ref.rs` lines 22-27: ULIDs sort lexicographically by creation time, which makes deterministic scene-cook (PLAN §1.6.10) trivial — feed the cook a deterministic seed and `EntityId`s come out monotone. A bare `u64` would require a separate creation-order index; we lean on the canonical scheme.

The 7 unit tests pin: lossy-display format, deterministic display, distinguishes-distinct-ids, transparent-serde round-trip, `from_u128` / `to_u128` round-trips, NIL constant.

## 4. `AssetId` — content-addressed identifier (re-exported from `kernel/asset`)

Per `crates/rge-data/src/asset_ref.rs` line 25:

```rust
pub use rge_kernel_asset::{AssetId, AssetIdParseError};
```

Post-2026-05-06 reconciliation, `kernel/asset` is the single canonical owner of `AssetId` per Status.md "AssetId canonical owner reconciliation"; rge-data is one of the seven downstream crates that swung from a local `pub struct AssetId` to a `pub use`. The migration was mechanical (per the module-doc lines 14-23):

- `as_bytes()` → `raw()`
- `from_content` / `from_digest` → `from_bytes` / `from_raw`
- `display()` → `to_string()`
- `WrongLength(n)` / `NonHex(n)` parse errors → `BadLength { expected, got }` / `BadHexChar(char)`

The 10 unit tests in `asset_ref.rs` pin (against the re-exported type): content stability for same source bytes, distinct-content discrimination, blake3-empty-input vector (`af1349b9f5...`), display-format prefix + 64-hex + lowercase, `FromStr` round-trip, missing-prefix rejection.

Cross-ref `KERNEL_ASSET.md` §2 for the canonical owner story; `PAK_FORMAT.md` §8 for the parallel re-export from pak-format.

## 5. `Project` — `.rge-project` schema

Lives at `crates/rge-data/src/project.rs`. Top-level container per PLAN §1.6.6:

```rust
pub struct Project {
    pub version: SchemaVersion,
    pub name: String,
    pub description: String,
    pub target_tiers: Vec<TargetTier>,    // Desktop / Mobile / Web / Headless
    pub plugins: Vec<PluginRef>,          // { id, version_req }
    pub scenes: Vec<ScenePath>,           // relative paths to .rge-scene
}

#[serde(rename_all = "lowercase")]
pub enum TargetTier { Desktop, Mobile, Web, Headless }

pub struct PluginRef { pub id: String, pub version_req: String }

#[serde(transparent)]
pub struct ScenePath(pub String);
```

`ScenePath` is `#[serde(transparent)]` so on-disk RON looks like `"scenes/main-menu.rge-scene"` rather than `(path: "...")`. `TargetTier` mirrors PLAN §0.1 (four pillars) + §1.6.4 (cooked binary). Plugin resolution / signing / capability gating is `crates/marketplace`'s problem; this struct is the on-disk pointer only.

3 unit tests pin: `Project::empty(name, version)` constructor, RON round-trip on a populated value, schema version field captured.

## 6. `Scene` — `.rge-scene` schema

Lives at `crates/rge-data/src/scene.rs`. The unit of authoring inside a project:

```rust
pub struct Scene {
    pub version: SchemaVersion,
    pub name: String,
    pub entities: Vec<Entity>,
    pub root_entities: Vec<EntityId>,    // entries with no ChildOf parent
}

pub struct Entity {
    pub id: EntityId,
    pub name: String,
    pub components: Vec<ComponentValue>,
    pub relations: Vec<Relation>,
}

pub struct ComponentValue {
    pub type_id: String,    // canonical type path (matches kernel/types::TypeId::path)
    pub data: String,       // RON literal that serializes the component's payload
}

pub enum Relation { ChildOf { parent: EntityId }, /* ... */ }
```

`ComponentValue` is the **reflection-neutral envelope** that lets `rge-data` carry components without a Reflect dep. Two-phase deserialise: parse the scene first (typed `Scene` struct), then walk each `ComponentValue::data` string through `kernel/types::serde_bridge::from_ron` to obtain a strongly typed `dyn Reflect` instance keyed by `ComponentValue::type_id`. That two-phase split is exactly what `kernel/types` already exposes.

`Scene::empty(name, version)` is the canonical constructor; `find_entity(id)` is a linear scan (fine for editor-time lookups; runtime uses `kernel/ecs` storage).

## 7. `Prefab` — `.rge-prefab` schema

Lives at `crates/rge-data/src/prefab.rs`. Reusable, parameterizable bundle of entities:

```rust
pub struct Prefab {
    pub version: SchemaVersion,
    pub name: String,
    pub parameters: Vec<ParamSpec>,
    pub entities: Vec<Entity>,                 // re-uses scene::Entity
    pub exposed_overrides: Vec<ExposedOverride>,
}

pub struct ParamSpec {
    pub name: String,                          // "color", "max_health"
    pub ty: String,                            // matches kernel/types::TypeId::path
    pub default: String,                       // RON literal
}

pub struct ExposedOverride {
    pub entity: EntityId,
    pub component_type: String,
    pub field_path: String,                    // dot-separated, "translation" / "color.r"
}
```

Looks like a `Scene` but additionally carries typed parameter knobs (`parameters` — the prefab's substitute for inheritance) and the set of `(entity_id, component_field)` pairs a parent scene is allowed to override (`exposed_overrides`). The runtime never instantiates a prefab directly; the editor / asset pipeline expands it into runtime entities and emits a "flat" scene under the hood.

## 8. RON wire format — discipline + canonical pretty-print

Per `lib.rs` lines 31-39:

- **Every source file opens with `version: "x.y.z"`** (PLAN §1.6.7).
- **Loader walks the migration chain** to bring the payload up to the current schema before deserialisation succeeds.
- **RON is the single source-format family** (PLAN §1.6.1); no JSON, no YAML, no binary fallback at this layer.
- **Round-trip RON → struct → RON is byte-identical** for files at the current schema (verified by `tests/round_trip.rs`).

The canonical `pretty_config()` (defined once in `tests/round_trip.rs` lines 27-35 so production loaders, migrations, and tests all use the same formatting):

```rust
fn pretty_config() -> ron::ser::PrettyConfig {
    ron::ser::PrettyConfig::new()
        .depth_limit(64)
        .new_line("\n".to_string())
        .indentor("    ".to_string())
        .struct_names(true)
        .separate_tuple_members(false)
        .enumerate_arrays(false)
}
```

The single named function is load-bearing: if a future wave changes formatting, every fixture has to be regenerated *and* the change is visible in this single function — no hidden divergence.

## 9. Migration substrate — versioned schema with chain walker

Lives at `crates/rge-data/src/migration.rs`. PLAN §1.6.7 commits to per-file `version: "x.y.z"` headers driving migration on load. The substrate provides:

```rust
pub trait Migration: fmt::Debug + Send + Sync {
    fn from_version(&self) -> SchemaVersion;
    fn to_version(&self) -> SchemaVersion;
    fn file_kind(&self) -> FileKind;
    fn apply(&self, ron_text: &str) -> Result<String, MigrationError>;
}

pub enum FileKind { Project, Scene, Prefab }

pub struct MigrationRegistry { /* private; ordered list */ }

pub fn migrate(
    registry: &MigrationRegistry,
    kind: FileKind,
    from: SchemaVersion,
    to: SchemaVersion,
    ron_text: &str,
) -> Result<String, MigrationError>;
```

### Why text-level (not typed `From<Old> for New`)

Per `migration.rs` lines 21-29: migrations operate on **RON text** rather than typed structs. A migration may rename a field, change a type, or split a single value into a tuple — none of which a strongly-typed `From<Old> for New` impl can express without keeping every old struct definition around forever. Working at the RON-AST level (via `ron::Value`) lets each migration perform a focused, surgical edit and dispose of the AST when it's done.

### File-kind tagging

Project, scene, and prefab files all share `version: "x.y.z"` but their other fields differ. The registry tags each `Migration` with a `FileKind` so a v0.1 → v0.2 *scene* migration is never accidentally applied to a *project*.

### v0.0 → v0.1 baseline (the only registered migration today)

`builtin::AddVersionField { kind: FileKind }` adds an explicit `version: "0.1.0"` field to a v0.0 file that lacks one (idempotent — already-versioned files pass through unchanged). `MigrationRegistry::with_builtin()` ships three instances (one per `FileKind`) so editors / cookers don't need custom migrations to load v0.0 fixtures.

The chain walker is greedy O(N²) (fine for small registries): at each step pick the migration whose `from_version` matches the cursor and whose `to_version` is closest to (but not past) `to`. Future v0.1 → v0.2 migrations register on top of the baseline.

### `MigrationError` taxonomy

```rust
pub enum MigrationError {
    InputParse(String),                                              // source RON failed to parse
    OutputParse { from, to, kind, reason: String },                  // migration produced invalid RON
    NoChain { from, to, kind },                                      // no path
    InconsistentChain { step, expected, actual },                    // registered chain has a gap
    Downgrade { from, to },                                          // from > to
    Custom { from, to, kind, reason: String },                       // user-supplied body returned an error
}
```

Every step's output must parse as RON before chain advance — corruption surfaces at the earliest possible point.

## 10. Round-trip stability gate

`tests/round_trip.rs` (10 tests) is the canonical exit-criterion check per the W14 dispatch package: load each of `tests/fixtures/sample_{project,scene,prefab}.rge-{project,scene,prefab}`, deserialise into the canonical struct, re-serialise with the canonical `pretty_config()`, and assert the bytes are identical to what the fixture says on disk.

The three fixtures exercise the full surface: `sample_project.rge-project` (multi-target-tier with plugin refs and scene paths); `sample_scene.rge-scene` (multi-entity hierarchy with components + relations); `sample_prefab.rge-prefab` (parameters + exposed-overrides over an entity tree).

The byte-identical RON round-trip is the foundation that asset-store cooked-pak determinism (`PAK_FORMAT.md` §1) depends on: pak-format serialises a `Scene` as `AssetKind::Scene` payload bytes; if the source RON had non-deterministic serialisation, the pak's blob bytes would vary across cooks even with identical input — breaking the §13.4 byte-identical-cook gate.

## 11. Test coverage breakdown — 81 tests

### Unit tests in `src/` (45 total)

- `schema_version.rs` (10): ordering-lexicographic, major-compat, display-dotted-triple, parse basic / too-few / too-many / non-numeric / overflow, RON quoted-string round-trip, default-zero.
- `entity_ref.rs` (7): NIL, lossy-`Display`-prefix, `Display` deterministic, `Display` distinguishes-distinct-ids, transparent-serde round-trip, `from_u128`/`to_u128` round-trip, canonical-form length.
- `asset_ref.rs` (10): content-stable, content-distinct, empty-blake3 known vector, display-prefix + 64-hex + lowercase, FromStr round-trip, missing-prefix rejection, plus 4 more on the re-exported `AssetIdParseError` variants.
- `project.rs` (3): `empty` constructor, RON round-trip, schema version field.
- `scene.rs` (3): `empty` constructor, `find_entity` linear scan, RON round-trip with components and relations.
- `prefab.rs` (2): `empty` constructor, RON round-trip with parameters + exposed overrides.
- `migration.rs` (10): registry-with-builtin returns 3 migrations, chain v0.0 → v0.1 has 1 step, equal-to-equal returns empty, rejects-downgrade, no-path-errors, passes-through-when-equal, rejects-unparseable-input, AddVersionField inserts when missing, AddVersionField idempotent when present, full v0.0 → v0.1 lossless on text round-trip.

### Integration tests in `tests/` (36 total)

- `migration_test.rs` (10): v0.0 fixture has no version field, migrate adds version field, lossless preserves body (full RON deserialise + per-field check), idempotency on re-run, plus 6 more covering chain walker corner cases against the vendored fixture.
- `round_trip.rs` (9): one-per-fixture RON → struct → RON byte-identical (3); plus 6 negative-case tests (corrupt fixture rejections, version-mismatch rejection, etc.).
- `schema_validation.rs` (17): identity-scheme + content-addressing + version-field invariants per the W14 exit-criteria checklist (EntityId Display format, AssetId blake3 stability, schema_version round-trip on each file kind, etc.).

Together the 81 tests cover the round-trip-stability gate (foundational for cook determinism), the schema-versioned migration substrate (with text-AST-level transformation discipline), the identity-scheme invariants (EntityId ULID + AssetId blake3), and the canonical pretty-print config that production loaders share with tests.

## 12. AssetId integration — `pub use rge_kernel_asset::AssetId`

Per `asset_ref.rs` line 25 (already covered in §4 above). The migration was mechanical, no data conversion needed — same wire form (`"blake3:<64-hex-lowercase>"`), same content-addressing semantics, same parser. The reconciled API surface used by rge-data:

- `AssetId::from_bytes(&[u8])` — content-addressing constructor.
- `AssetId::from_raw([u8; 32])` — bypass for the wire-decoded case.
- `AssetId::to_string() -> String` — the canonical `"blake3:<hex>"` text form for serde.
- `AssetId::cmp(...)` — total order; underlies serde's deterministic emission.

Cross-ref `KERNEL_ASSET.md` §2 for the canonical owner story; `PAK_FORMAT.md` §8 for the parallel re-export from pak-format.

## 13. Cross-ref to `PAK_FORMAT` — rge-data schemas as pak payloads

`pak-format` carries an `AssetKind` enum (`PAK_FORMAT.md` §5) with three values that match rge-data file kinds:

| `AssetKind` | rge-data type | wire format inside the pak blob |
|---|---|---|
| `AssetKind::Scene` (= 8) | `rge_data::Scene` | RON-serialised, then zstd-compressed |
| `AssetKind::Prefab` (= 9) | `rge_data::Prefab` | RON-serialised, then zstd-compressed |
| `AssetKind::Project` is **not** an `AssetKind` value | — | — |

(Project is the **outer** container; it lives on disk at the root of the project tree, NOT inside a cooked pak. Cooked paks ship Scene + Prefab payloads keyed by `AssetId` content-derived from the RON bytes.)

The cooked-pak load flow:
1. Asset-store opens the `.rge-pak` via `PakReader::open`.
2. `pak.first_of_kind(AssetKind::Scene)` returns the cooked scene blob.
3. The blob is the RON-serialised `Scene` bytes; `ron::from_str` deserialises into the typed struct.
4. If the blob's schema version is older than current, `migrate(registry, FileKind::Scene, blob_version, current, ron_text)` brings it forward before deserialise.

Determinism propagates: same source `Scene` → same canonical RON → same `AssetId` (content-derived from the RON bytes) → same pak position. The byte-identical-cook gate (`PAK_FORMAT.md` §1) is therefore a property of the rge-data canonical pretty-print + the pak-format writer's sort-and-zstd pipeline acting in concert.

## 14. Failure class — recoverable

`crates/rge-data/src/lib.rs` does **not** currently carry a `//! Failure class: <kind>` declaration; the crate appears in `tools/architecture-lints/exemptions.toml` (line 272):

```toml
[[exemption]]
lint = "failure-class"
file = "crates/rge-data/Cargo.toml"
reason = "Phase 1.x rollout debt - declaration added when crate gets first real implementation per IMPLEMENTATION.md."
```

The exemption is rollout-debt per the audit-1 deal (`RECOVERY_MODEL.md` §6); rge-data **is** implemented (not stub), but the declaration was not added when W14 landed and is tracked for cleanup as part of the 58 remaining failure-class rollout-debt entries.

The crate's intended class is **recoverable**: every `MigrationError` / `SchemaVersionParseError` / `AssetIdParseError` / RON deserialise failure is caller-recoverable — the loader branches on the variant and surfaces actionable diagnostics (which file, which version, which migration step). Schema corruption does NOT escalate to snapshot-recoverable because the recovery path is "fix the file or roll back to a backup", not "restore the engine's state".

## 15. Source / spec inconsistencies

- **Brief stated `Phase 4.3 — crates/rge-data | shipped W14 — 80 tests`**; source-truth via `grep -c '#\[test\]'`: 81 test attributes total (45 unit + 36 integration). Status.md line 49 also says "rge-data | 81". The brief's "80" appears to be an off-by-one; the doc reflects the live count.
- **Brief stated "AssetId integration (rge-data uses `pub use rge_kernel_asset::AssetId;`)"**; source-truth confirmed at `asset_ref.rs` line 25. The reconciliation closure in Status.md "AssetId consumer migration" (line 27) confirms rge-data is one of the seven crates that migrated. The doc reflects the migration verbatim per the module-doc lines 14-23.
- **Brief stated "Project / Scene / Prefab schema types"**; source-truth confirmed: three top-level structs in `project.rs` / `scene.rs` / `prefab.rs`. The brief did not mention the supporting types (TargetTier / PluginRef / ScenePath / Entity / ComponentValue / Relation / ParamSpec / ExposedOverride) — the doc covers them since they're load-bearing for round-trip understanding.
- **Brief assumed rge-data declares a `//! Failure class:` value**; source-truth: rge-data does NOT carry the declaration (rollout-debt exemption still in place). The doc surfaces this honestly under §14 rather than papering over with an inferred class.
- **Brief stated "Migration substrate (versioned schema; backward-compat strategy)"**; source-truth: only the **forward** v0.0 → v0.1 baseline migration is registered today; backward migration (downgrade) is explicitly rejected with `MigrationError::Downgrade` in `MigrationRegistry::chain`. The "backward-compat strategy" is therefore: on load, walk the chain forward to bring the payload up to current; on save, always emit at the current schema version. There is no in-place backward-migration path. The doc reflects this in §9.

## 16. References

- **PLAN.md §1.6** — file format discipline; the project / scene / prefab triplet is the §1.6.6 commitment.
- **PLAN.md §1.6.1** — RON as the single source-format family (no JSON / YAML / binary alternative).
- **PLAN.md §1.6.3** — identity scheme: ULID-based `EntityId`, content-addressed `AssetId`.
- **PLAN.md §1.6.6** — project layout subtree shape (assets / scenes / prefabs / materials / scripts / plugins / target/cook).
- **PLAN.md §1.6.7** — versioning + migration; the per-file `version: "x.y.z"` header + chain-walker contract.
- **PLAN.md §1.6.10** — cook determinism; rge-data's byte-identical RON round-trip is the substrate pak-format builds on.
- **IMPLEMENTATION.md Phase 4.3** — `crates/rge-data` shipped W14; this doc lifts the W14 dispatch package's schema spec into a §18 reference.
- **`KERNEL_ASSET.md`** — sibling §18 doc; canonical `AssetId` owner rge-data re-exports per the 2026-05-06 reconciliation.
- **`PAK_FORMAT.md`** — sibling §18 doc; the cooked-pak container that serialises rge-data Scene + Prefab payloads as content-addressed `AssetKind::{Scene, Prefab}` blobs.
- **`crates/rge-data/src/lib.rs`** — module roots + invariants paragraph + local `Reflect` stub (until W02 lands).
- **`crates/rge-data/src/schema_version.rs`** — `SchemaVersion` u8-triple + custom serde for quoted-string wire form + 10 unit tests.
- **`crates/rge-data/src/entity_ref.rs`** — `EntityId(Ulid)` with lossy `Display` + 26-char canonical serde + 7 unit tests.
- **`crates/rge-data/src/asset_ref.rs`** — `pub use rge_kernel_asset::{AssetId, AssetIdParseError};` + 10 unit tests.
- **`crates/rge-data/src/project.rs`** — `Project` + `TargetTier` + `PluginRef` + `ScenePath` + 3 unit tests.
- **`crates/rge-data/src/scene.rs`** — `Scene` + `Entity` + `ComponentValue` + `Relation` + 3 unit tests.
- **`crates/rge-data/src/prefab.rs`** — `Prefab` + `ParamSpec` + `ExposedOverride` + 2 unit tests.
- **`crates/rge-data/src/migration.rs`** — `Migration` trait + `MigrationRegistry` + `migrate` chain walker + `MigrationError` + `builtin::AddVersionField` + 10 unit tests.
- **`crates/rge-data/tests/round_trip.rs`** — 9-test round-trip-byte-identical gate against the three vendored fixtures + canonical `pretty_config()`.
- **`crates/rge-data/tests/migration_test.rs`** — 10-test v0.0 → v0.1 lossless migration regression + idempotency.
- **`crates/rge-data/tests/schema_validation.rs`** — 17-test identity-scheme + content-addressing + version-field invariant suite per the W14 exit-criteria checklist.
