# KERNEL_TYPES

| Companion to | PLAN.md §1.2.4 (reflection registry as architectural root) + PLAN.md §6.15 (UI-hint closed-set vocabulary) + PLAN.md §1.10 (hand-rolled vs dep-pull discipline) + PLAN.md §1.13 (snapshot-recoverable failure class) + IMPLEMENTATION.md Phase 1.1 (kernel-types exit criteria) |
|---|---|
| Status | Stable v1; 24 tests passing (21 unit tests across `type_id` / `schema_version` / `field_descriptor` / `ui_hint` / `serde_bridge` / `reflect` modules + 3 integration tests in `kernel/types/tests/reflect_round_trip.rs` covering RON byte-identity for the `RenderPass`-shaped pilot fixture); first Tier-1 crate to land per IMPLEMENTATION.md Phase 1.1; consumed by `crates/macros-reflect` (the `#[derive(Reflect)]` macro), `crates/components-spatial`, `crates/script-host`, and `crates/rge-data` today |
| Audience | Authors of types that must round-trip through RON / appear in the editor inspector / migrate across hot-reload (anyone reaching for `#[derive(Reflect)]`); reviewers verifying the architectural-root invariants (zero blake3, zero `inventory!`, no global runtime registry) |
| Sibling doc | `KERNEL_DIAGNOSTICS.md` — `SerdeBridgeError::SchemaMismatch` is auto-classified as snapshot-recoverable per PLAN §1.13; `KERNEL_AUDIT_LEDGER.md` — `EventId` derivation uses BLAKE3, distinct from `kernel/types::TypeId`'s hand-rolled FNV-1a-128 (the divergence is intentional — see §3); subsystems holding `Reflect`-derived types reference this doc as the substrate |
| Reference impls | `kernel/types/src/{lib,type_id,schema_version,field_descriptor,ui_hint,reflect,serde_bridge}.rs` (substrate) · `kernel/types/tests/reflect_round_trip.rs` (RenderPass pilot) · `kernel/types/BUDGET.md` (compile-time budget for 5 pilot types) · `crates/macros-reflect/src/codegen.rs` (consumer; the `#[derive(Reflect)]` macro emits `impl Reflect` against this substrate) |

> Convention defined by `PLUGIN_HOST_PATTERNS.md` §header. This doc is the workspace-wide reference for the reflection-registry substrate; each downstream Tier-2 crate that derives `Reflect` for its types may carry a sibling §18 doc covering its consumer-side conventions (e.g. `EDITOR_STATE_MODEL.md` for editor inspector binding).

## 1. Why this crate is the architectural root

> **Source-truth flag (load-bearing):** the dispatch spec described `kernel/types` as the home of "Span / SourceLoc / scalar wrappers / enums shared across kernels" and as a "universal Tier-1 dep". Source-truth: `kernel/types` is the **reflection-registry substrate** — `TypeId`, `FieldDescriptor`, `Reflect` trait, `UiHint`, `SchemaVersion`, `serde_bridge`. `Span` and `SourceLoc` live in `kernel/diagnostics` (see `KERNEL_DIAGNOSTICS.md` §5), not here. The crate is **not** a universal Tier-1 dep — only ~3 Tier-2 crates currently consume it (macros-reflect, components-spatial, script-host). This doc reflects the actual surface; the brief's framing was speculative and is corrected here as the canonical reference.

The crate is the architectural root in the sense that **every later subsystem that needs to walk values structurally** reaches through this substrate's `Reflect` trait. From `kernel/types/src/lib.rs`:

> Per `IMPLEMENTATION.md` Phase 1.1 and `PLAN.md` §1.2.4 / §6.15: every later subsystem (editor inspector, hot-reload migration, scripting bridge, asset metadata, RON serde for project files) walks values through `Reflect`.

Concretely: the editor inspector binds widgets to fields via the `UiHint` enum on each `FieldDescriptor`; the `kernel/asset` migration table (Phase 2.4) walks `Reflect::SCHEMA_VERSION` to route major-version-mismatch payloads through their migration entries; the future `runtime-wasmtime` scripting bridge marshals values through the `ReflectValue` sum type; project-file RON serde reaches the same substrate via the `from_ron` / `to_ron` helpers in `serde_bridge`.

The "architectural" qualifier means **the reflection layer cannot be slow**: the compile-time gate is that 5 pilot types compile in <30s (see `kernel/types/BUDGET.md`). The crate is therefore engineered for minimal compile-time cost (no proc-macro expansion in the trait surface itself; the macro lives separately in `crates/macros-reflect`) and minimal binary cost (no runtime registry; every `FieldDescriptor` is a `&'static` const).

## 2. Tier-1 layering — the actual rule

> **Source-truth flag:** the dispatch spec described a "forbidden-dep lint rule that lets ALL Tier-1 crates depend on kernel/types". Source-truth: there is **no such special-case rule** in `tools/architecture-lints/src/forbidden_dep.rs`. The lint enforces six general rules (Tier-1 cannot depend on Tier-2; Tier-2 cannot depend on Tier-3; cad-core stands alone; editor-ui cannot depend on physics/audio/input; physics cannot depend on script-host; renderer cannot depend on game-domain). `kernel/types` is one Tier-1 crate among ~14 others; any other Tier-1 may depend on it (no rule forbids that), and any Tier-2 may depend on it (Tier-2 → Tier-1 is the default direction). The crate's "architectural root" status comes from its consumer pattern (every reflection consumer reaches through here), not from a dedicated lint exemption.

The crate's Tier-1 status follows from its directory location (`kernel/*` is Tier-1 per the `forbidden-dep` lint's `classify` helper). Its dependency floor is the workspace minimum:

```toml
[dependencies]
serde      = { workspace = true }
ron        = { workspace = true }
thiserror  = { workspace = true }
```

No `blake3`, no `inventory`, no `linkme`, no `paste`. This is a **deliberate** floor — see §3.

## 3. Hand-rolled FNV-1a-128 (not BLAKE3)

The architectural-root discipline drives one of the crate's more visible design choices: `TypeId` uses a hand-rolled 128-bit hash, not BLAKE3.

```rust
// kernel/types/src/type_id.rs
const FNV_OFFSET_LO: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME_LO:  u64 = 0x0000_0100_0000_01b3;
const FNV_OFFSET_HI: u64 = 0x9ae1_6a3b_2f90_404f;
const FNV_PRIME_HI:  u64 = 0x0000_0100_0000_0193;
```

Two parallel FNV-1a runs over the bytes (with different prime constants so the halves are independent) produce two 64-bit halves; concatenated they yield a 128-bit id. Domain separator `b"rge::TypeId/v1\0"` prevents accidental cross-domain collision with other content-hashed inputs.

The lib.rs explains the choice:

> The architectural root must keep its dependency floor at the workspace minimum (`PLAN.md` §1.10 last metric — incremental invalidation radius). Pulling `blake3` would drag in `cpufeatures 0.3.0` which currently requires `edition2024` (not stabilized in our pinned 1.78 toolchain). Type ids are not security-sensitive — they are stability anchors.

Contrast `kernel/audit-ledger::EventId`, which **does** use BLAKE3 (32 bytes, content-derived from `(kind_tag, payload)`). The substrates intentionally diverge: BLAKE3 for content-addressed event identity (cross-process dedup; cryptographically meaningful); FNV-1a-128 for content-derived **type** identity (~2^-30 collision probability at 10^4 reflected types — well below the threshold where a cryptographic hash would matter).

## 4. `TypeId` — content-derived stable type identity

Lives at `kernel/types/src/type_id.rs`.

```rust
pub struct TypeId([u8; 16]);

impl TypeId {
    pub fn of_name(name: &str) -> Self;
    pub const fn from_bytes(bytes: [u8; 16]) -> Self;
    pub const fn as_bytes(&self) -> &[u8; 16];
    pub fn to_hex(&self) -> String;
}
```

`of_name` hashes a fully-qualified path (e.g. `"crate_root::module::TypeName"`) into a stable 128-bit id. The derive macro emits `module_path!() ++ "::" ++ stringify!(Ident)` so two types in different modules of the same crate are distinguishable. `from_bytes` is the const constructor for hand-built ids (used by built-in primitive ids and tests).

Properties:

- **Stable across builds.** Pure hash of the name string — reproducible across machines, toolchain bumps, incremental recompiles. Asset files written today round-trip against builds tomorrow.
- **Stable across processes.** Asset files, audit-ledger entries, project schemas can store and compare ids.
- **Content-derived.** No global registry, no atomic counter — the language already forbids two reflected types in different crates sharing the same fully-qualified path.

`std::any::TypeId` is rejected because its bytes are not contractually stable across builds (rust-lang/rust#80377).

The 7 unit tests pin: distinct names produce distinct ids, same name produces same id across calls, module-path disambiguation, hex round-trip via `Display`, RON serde round-trip, empty-name legality, long-name handling.

## 5. `SchemaVersion` — every reflected type carries one

Lives at `kernel/types/src/schema_version.rs`.

```rust
pub struct SchemaVersion {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

impl SchemaVersion {
    pub const fn new(major: u16, minor: u16, patch: u16) -> Self;
    pub const UNVERSIONED: Self;                                       // (0, 0, 0)
    pub const fn is_compatible_with(self, other: Self) -> bool;        // major-equal
}
```

A SemVer-flavoured triple stored alongside the reflected payload. The bumping policy (lib.rs):

- **Patch** — non-breaking field rename via `#[reflect(serde_alias = "old")]`.
- **Minor** — additive field with default; old data still loads.
- **Major** — incompatible change; migration entry required in the `kernel/asset` migration table (Phase 2.4 — not yet built).

Phase 1.1 only enforces the constant's presence + serde shape. Migration **routing** belongs to a later wave; today the bridge only detects mismatch and surfaces `SerdeBridgeError::SchemaMismatch`.

The 4 unit tests pin: lexicographic ordering across (major, minor, patch), major-compat predicate, `Display` (`"1.2.3"`), RON round-trip.

## 6. `Reflect` — trait emitted by `#[derive(Reflect)]`

Lives at `kernel/types/src/reflect.rs`.

```rust
pub trait Reflect: Sized {
    const TYPE_ID: TypeId;
    const TYPE_NAME: &'static str;
    const FQ_TYPE_NAME: &'static str;
    const SCHEMA_VERSION: SchemaVersion;
    const FIELDS: &'static [FieldDescriptor];
    const KIND: ReflectKind;

    fn get_field_dyn(&self, name: &str) -> Result<ReflectValue, ReflectError>;
    fn set_field_dyn(&mut self, name: &str, value: ReflectValue) -> Result<(), ReflectError>;
}
```

Six const items + two dynamic accessors. The const items make `Reflect` itself **not** object-safe — a `dyn Reflect` would have to vtable-dispatch the constants. The crate provides `ReflectObject` as the object-safe shadow (with `fn`-shaped accessors mirroring each const) plus a blanket `impl<T: Reflect> ReflectObject for T` so `&dyn ReflectObject` works for any reflected type:

```rust
pub trait ReflectObject {
    fn field_get(&self, name: &str) -> Result<ReflectValue, ReflectError>;
    fn field_set(&mut self, name: &str, value: ReflectValue) -> Result<(), ReflectError>;
    fn reflect_type_id(&self) -> TypeId;
    fn reflect_type_name(&self) -> &'static str;
    fn reflect_schema_version(&self) -> SchemaVersion;
    fn reflect_fields(&self) -> &'static [FieldDescriptor];
    fn reflect_kind(&self) -> ReflectKind;
}
```

The method names differ (`get_field_dyn` vs `field_get`) to prevent method-resolution ambiguity at user call-sites that import both traits.

`ReflectKind` forward-declares Phase-2 enum / tuple-struct support:

```rust
pub enum ReflectKind { NamedStruct, TupleStruct, Enum }
```

Phase 1.1 only supports `NamedStruct`; the macro rejects tuple-structs and enums with a clear error.

`ReflectError` carries three variants: `UnknownField(&'static str)`, `TypeMismatch { field, expected, got }`, `SkippedField(&'static str)`. Skipped fields are still readable (the inspector may show them as read-only) but cannot be set via reflection.

## 7. `FieldDescriptor` — per-field metadata (no global registry)

Lives at `kernel/types/src/field_descriptor.rs`.

```rust
pub struct FieldDescriptor {
    pub name: &'static str,         // stringify!(field_ident)
    pub ty_name: &'static str,      // source-level type spelling
    pub ty_id: TypeId,              // <FieldType as Reflect>::TYPE_ID
    pub range: Option<RangeMeta>,   // inclusive numeric range (cross-checked vs UiHint::Slider)
    pub default: DefaultValue,      // closed-set default
    pub ui_hint: UiHint,            // inspector binding hint
    pub serde_skip: bool,           // skip in serde round-trip
}
```

Each reflected struct exposes a `&'static [FieldDescriptor]` slice via `Reflect::FIELDS`. Two consequences:

1. **Lookup is fully inlinable.** No global registry round-trip; the inspector / serde bridge / migration walker see a const slice known at compile time.
2. **`no_std`-eligible.** When the future Phase-5 wasm runtime profile lands, the reflection layer can be used without a heap allocator. The `&'static` discipline is load-bearing for that.

`RangeMeta { min: f64, max: f64 }` is the inclusive numeric range cross-checked against `UiHint::Slider` (the inspector lint flags a `Slider` on a `String` field per `UiHint::expects_numeric()`).

`DefaultValue` is a closed-set sum type: `DeriveDefault` (use `Default::default()`), `Required`, `Bool(bool)`, `Int(i64)`, `Float(f64)`, `String(&'static str)`, `Custom(&'static str)` (path to a `fn() -> T` for editor "Reset" actions). Closed-set rather than `String` so the migration layer can reason about defaults without re-parsing RON.

The crate's "no global registry at runtime" rule (lib.rs) is the discipline this descriptor enforces: a future `inventory!` crate is **explicitly out of scope** per PLAN §1.10's dynamic-island policy. Tooling that needs trait-object reflection uses `&dyn ReflectObject` against a hand-held value, not a global table.

## 8. `UiHint` — closed-set inspector vocabulary

Lives at `kernel/types/src/ui_hint.rs`. Twelve variants:

```rust
pub enum UiHint {
    Default,
    Slider { min: f64, max: f64, step: f64 },
    ColorRgb,
    ColorRgba,
    FilePath { extensions: &'static [&'static str] },
    EnumDropdown,
    Multiline { lines: u16 },
    Curve,
    Gradient,
    Foldout { default_open: bool },
    Inline,
    Hidden,
}
```

Closed-set per PLAN §6.15 — adding `Custom(String)` is **forbidden** because that would defeat the lint that polices the vocabulary. Custom drawers are wired through `#[reflect(custom_drawer = "fn_path")]` on the field, not through this enum.

Each variant carries its own payload (slider min/max/step; FilePath extension whitelist; Multiline visible-row count; Foldout default-open flag) rather than using a separate metadata bag — so the inspector's lookup is one `match` on the descriptor's `ui_hint` field.

`expects_numeric()` returns true for `Slider` (the W08 inspector lint that flags a `Slider` on non-numeric fields uses this). `hides_in_inspector()` returns true for `Hidden`. The 5 unit tests pin the variant defaults, predicate semantics, and serde-out shape (the enum is **`Serialize`-only** because variants like `FilePath { extensions: &'static [&'static str] }` cannot round-trip through `Deserialize<'de>: 'static`; the macro emits these as const literals).

## 9. `serde_bridge` — RON round-trip via reflection walk

Lives at `kernel/types/src/serde_bridge.rs`. The closed-set sum type for dynamic field IO:

```rust
pub enum ReflectValue {
    Bool(bool),
    I64(i64),     // any signed integer; coerced
    U64(u64),     // any unsigned; coerced
    F64(f64),     // any float; coerced
    String(String),
    StaticStr(&'static str),    // macro-emitted const default paths
    Unit,
}
```

The crate **deliberately avoids `dyn Any` downcasts** for field access — `set_field_dyn` works on this purpose-shaped sum, not `dyn Any`. From the lib.rs forbidden-patterns list: this keeps the surface auditable + allows the `#![cfg_attr(not(test), forbid(unsafe_code))]` lint to stay clean.

The convenience helpers:

```rust
pub fn to_ron<T: Reflect + Serialize>(value: &T) -> Result<String, SerdeBridgeError>;
pub fn from_ron<T: Reflect + DeserializeOwned>(s: &str) -> Result<T, SerdeBridgeError>;
pub fn to_ron_pretty<T: Reflect + Serialize>(value: &T) -> Result<String, SerdeBridgeError>;
```

The Phase 1.1 scope is deliberately bounded: the pilot test (`reflect_round_trip.rs`) uses a fixture that derives both `Reflect` AND `Serialize/Deserialize`; the **fully reflection-driven serializer** (no `Serialize` derive) is a Phase-2 deliverable. Today these helpers prove the round-trip is achievable via the compile-time descriptors; the byte-identity claim is pinned by the pilot:

```text
serialize(original) -> s1
deserialize(s1)     -> reconstructed
serialize(reconstructed) -> s2
assert s1 == s2  (byte-identical)
assert reconstructed == original (value-identical)
```

`SerdeBridgeError` carries: `Ron(#[from] ron::Error)`, `RonSpanned(String)` (forwarded from `ron::error::SpannedError`), `SchemaMismatch { type_name, on_disk, in_memory }`, `MissingField(&'static str)`. The `SchemaMismatch` variant is the one classified as **snapshot-recoverable** per PLAN §1.13 — the in-memory binary's `SCHEMA_VERSION.major` differs from the payload's, so the loaded data must route through the (Phase 2.4) migration table.

## 10. The 24-test coverage breakdown

The 24 tests split across two surfaces:

### Unit tests in `src/` (21 total)

- `type_id.rs` (7): distinct-names, same-name-stable, module-path-disambiguation, hex round-trip, RON serde, empty-name, long-name.
- `schema_version.rs` (4): lexicographic order, major-compat predicate, `Display` format, RON serde.
- `field_descriptor.rs` (3): const builder, `RangeMeta` round-trip, `DefaultValue` serializes-to-RON.
- `ui_hint.rs` (5): default-is-default, hidden predicate, numeric predicate, slider serializes, file-path with extensions.
- `serde_bridge.rs` (1): variant-names-distinct.
- `reflect.rs` (1): named-struct round-trip via `dyn ReflectObject`.

### Integration tests in `tests/reflect_round_trip.rs` (3 total)

- `render_pass_round_trips_byte_identically_via_ron` — the Phase 1.1 byte-identity exit criterion using a `RenderPass`-shaped fixture (8 fields: name / enabled / priority / clear_color_{r,g,b,a} / msaa_samples).
- `render_pass_reflection_descriptors_match_struct` — `FIELDS.len() == 8`, `TYPE_NAME == "RenderPass"`, `SCHEMA_VERSION.major == 1`, `KIND == NamedStruct`, descriptor names match struct field order.
- `dyn_field_mutation_round_trip` — `set_field_dyn("priority", I64(999))` mutates the value; subsequent `to_ron` + `from_ron` round-trip preserves the mutation.

The fixture deliberately uses **hand-written** `Reflect` impl (not the macro) so the test depends on `rge-kernel-types` alone. The macro-driven path is covered separately in `crates/macros-reflect/tests/derive_test.rs`.

## 11. Failure class — recoverable

`kernel/types/src/lib.rs` line 3 declares:

```rust
//! Failure class: recoverable
```

Per PLAN §1.13. The crate is a typed-IO substrate; its operations are infallible at the structural level (constructing a `TypeId` from a name; building a `FieldDescriptor`; comparing a `SchemaVersion`). The fallible operations live in `serde_bridge` and surface as `SerdeBridgeError` — the caller decides what to do:

- `Ron` / `RonSpanned` — parse failure; caller retries with corrected input or gives up.
- `MissingField` — a `Required`-default field was absent; caller can synthesise the field or reject the payload.
- `SchemaMismatch` — payload's major schema version differs from compiled-in; this is the **snapshot-recoverable** path that PLAN §1.13 routes through the (Phase 2.4) migration table. The `Diagnostic` carries the path; the routing layer decides whether to migrate, fall back to a snapshot, or surface an error to the user.

The crate itself never escalates — it surfaces `Result` and the caller decides. The `architecture-lints` `failure-class` lint enforces the lib.rs declaration; `kernel/types` does not appear in `tools/architecture-lints/exemptions.toml`.

## 12. Forbidden patterns (hard discipline)

The lib.rs lists two non-negotiables that any future contributor must respect:

- **No global registry at runtime.** Every reflected type exposes its `FieldDescriptor` slice as a `&'static` const. A future `inventory!` crate is explicitly out of scope (PLAN §1.10's dynamic-island policy: tooling layers can use `dyn ReflectObject` trait objects; no global table).
- **No `Any` downcast for field access.** `set_field_dyn` works on the purpose-shaped `ReflectValue` enum, not `dyn Any`. This keeps the surface auditable and the `unsafe_code = forbid` lint clean.

The `#![cfg_attr(not(test), forbid(unsafe_code))]` and `#![warn(missing_docs)]` attributes on the lib.rs reinforce the discipline.

## 13. Consumer surface

The substrate's downstream users today + tomorrow:

- **`crates/macros-reflect`** — the `#[derive(Reflect)]` proc macro. Emits `impl Reflect for T` against this substrate's trait. The macro's `derive_test.rs` is the canonical macro-driven fixture (paralleling the hand-rolled `reflect_round_trip.rs` here).
- **`crates/components-spatial`** — uses `#[derive(Reflect)]` on its `Entity`-component types so they appear in the editor inspector + round-trip through project files.
- **`crates/script-host`** — the scripting bridge marshals `ReflectValue` across the scripting boundary so scripts can read/write reflected fields.
- **`crates/rge-data`** — marshals reflected payloads through the data-layer abstractions.
- **Future: editor inspector** (Phase 4-W08) — walks `Reflect::FIELDS` to render widgets bound by `UiHint`.
- **Future: `kernel/asset` migration table** (Phase 2.4) — routes `SchemaMismatch` payloads through registered migration entries keyed on `(TypeId, on_disk_major)`.
- **Future: `runtime-wasmtime` scripting bridge** — marshals `ReflectValue` into the WASM ABI.

Crates that don't reflect their types (most kernel substrates today) don't depend on this crate; the architectural-root claim is qualitative (every reflection consumer reaches through here), not quantitative (every Tier-1/Tier-2 crate depends on it).

## 14. References

- **PLAN.md §1.2.4** — reflection registry as architectural root.
- **PLAN.md §6.15** — UI-hint closed-set vocabulary.
- **PLAN.md §1.10** — hand-rolled vs dep-pull discipline; the rule that drove the FNV-1a-128 (not BLAKE3) decision.
- **PLAN.md §1.13** — failure-class taxonomy; `SchemaMismatch` → snapshot-recoverable.
- **IMPLEMENTATION.md Phase 1.1** — exit criteria; the byte-identity round-trip is the gate.
- **`KERNEL_DIAGNOSTICS.md`** — sibling §18 doc; `Span` / `SourceLoc` live there, not here. `SerdeBridgeError` surfacing through the diagnostic substrate uses the `Recoverable` failure class.
- **`KERNEL_AUDIT_LEDGER.md`** — sibling §18 doc; uses BLAKE3 for `EventId` (intentionally divergent from this crate's FNV-1a-128 — see §3 for the design rationale).
- **`kernel/types/src/lib.rs`** — module roots + failure-class declaration + design goals + forbidden-patterns list.
- **`kernel/types/src/type_id.rs`** — `TypeId` + 7 unit tests + the FNV-1a-128 construction.
- **`kernel/types/src/schema_version.rs`** — `SchemaVersion` + bumping policy + 4 unit tests.
- **`kernel/types/src/field_descriptor.rs`** — `FieldDescriptor` + `RangeMeta` + `DefaultValue` + 3 unit tests.
- **`kernel/types/src/ui_hint.rs`** — `UiHint` (12 variants, closed-set) + 5 unit tests.
- **`kernel/types/src/reflect.rs`** — `Reflect` trait + `ReflectObject` shadow + `ReflectKind` + `ReflectError` + 1 unit test (named-struct via `dyn ReflectObject`).
- **`kernel/types/src/serde_bridge.rs`** — `ReflectValue` + `to_ron` / `from_ron` / `to_ron_pretty` + `SerdeBridgeError` + 1 unit test.
- **`kernel/types/tests/reflect_round_trip.rs`** — 3 integration tests; the `RenderPass`-shaped pilot fixture is the Phase 1.1 byte-identity exit criterion.
- **`kernel/types/BUDGET.md`** — compile-time budget for 5 pilot types (<30s).
- **`crates/macros-reflect/src/codegen.rs`** — consumer; the `#[derive(Reflect)]` macro emits `impl Reflect` against this substrate.
