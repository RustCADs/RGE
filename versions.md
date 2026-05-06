# RGE — Workspace Versions

> **Last audit vs crates.io latest stable:** 2026-05-05.
> **Source of truth:** `Cargo.toml` `[workspace.dependencies]` + per-crate `[dependencies]`.
> **Automated re-audit:** TODO `tools/dependency-auditor` (currently stub).

## Toolchain

| Component | Pinned | Latest stable | MSRV driver |
|---|---|---|---|
| rust-toolchain | **1.92.0** | (rolling) | `egui_dock 0.19` requires 1.92 |
| edition | **2021** | 2024 (stable since 1.85) | held; many deps now use edition2024 transitively but workspace stays 2021 for now |
| Components | rustfmt, clippy, rust-src, rust-analyzer | — | — |
| Targets | x86_64 win/linux, aarch64 mac, wasm32 (unknown + wasip1) | — | per pillar 1 platform matrix |

---

## Workspace dependencies (direct)

### Substrate

| Crate | Pinned | Latest stable | MSRV | Notes |
|---|---|---|---|---|
| `serde` | `1` (→ 1.0.228) | **1.0.228** (Sep 2025) | 1.31 | derive feature on |
| `serde_json` | `1` (→ 1.0.149) | **1.0.149** (Jan 2026) | 1.68 | — |
| `ron` | `0.12` | **0.12.1** (Mar 2026) | — | bumped 0.8 → 0.12 (4 minors) |
| `toml` | `1` | **1.1.2+spec-1.1.0** (Apr 2026) | 1.85 | bumped 0.8 → 1 (major); requires edition2024 transitively |
| `thiserror` | `2` | **2.0.18** (Jan 2026) | 1.68 | bumped 1 → 2 (`#[from]` syntax change) |
| `anyhow` | `1` | **1.0.102** (Feb 2026) | 1.68 | latest |
| `tracing` | `0.1` (→ 0.1.44) | **0.1.44** (Dec 2025) | low | latest |
| `tracing-subscriber` | `0.3` (→ 0.3.23) | **0.3.23** (Mar 2026) | — | latest |

### Hashing / IDs

| Crate | Pinned | Latest stable | MSRV | Notes |
|---|---|---|---|---|
| `blake3` | `1.8` | **1.8.5** (Apr 2026) | 1.74 | bumped 1.5 → 1.8; cpufeatures 0.3 OK at toolchain 1.92 |
| `ulid` | `1` (→ 1.2.1) | **1.2.1** | — | scene EntityId source |

### Async

| Crate | Pinned | Latest stable | MSRV | Notes |
|---|---|---|---|---|
| `tokio` | `1` (→ 1.52.2) | **1.52.2** (May 2026) | 1.71 | features: rt, rt-multi-thread, macros, sync, time |

### Filesystem / hot-reload

| Crate | Pinned | Latest stable | MSRV | Notes |
|---|---|---|---|---|
| `notify` | `8` | **8.2.0** (Aug 2025) | — | bumped 6 → 8 (2 majors; `Config::with_*` builder shape changed) |

### Compression

| Crate | Pinned | Latest stable | MSRV | Notes |
|---|---|---|---|---|
| `zstd` | `0.13` (→ 0.13.3) | **0.13.3** (Feb 2025) | 1.64 | latest |

### WASM substrate

| Crate | Pinned | Latest stable | MSRV | Notes |
|---|---|---|---|---|
| `wasmtime` | `44` | **44.0.1** (Apr 2026) | — | bumped 23 → 44 (21 majors); `cache` feature off (windows-sys 0.52 conflict) |
| `wit-bindgen` | `0.57` | **0.57.1** (Apr 2026) | — | bumped 0.30 → 0.57 (paired with wasmtime 44) |
| `wasm-encoder` | `0.248` | **0.248.0** (Apr 2026) | — | added; paired with wasmtime 44 / wit-bindgen 0.57 release train |

### Graphics

| Crate | Pinned | Latest stable | MSRV | Notes |
|---|---|---|---|---|
| `wgpu` | `29` | **29.0.3** (May 2026) | — | bumped 22 → 29 (7 majors); pairs with naga 29 |
| `naga` | `29` | **29.0.3** (May 2026) | — | matches wgpu |

### UI (egui release-train — all four crates pin to same minor)

| Crate | Pinned | Latest stable | MSRV | Notes |
|---|---|---|---|---|
| `egui` | `0.34` | **0.34.2** (May 2026) | — | bumped 0.27 → 0.34 (7 minors) |
| `egui-winit` | `0.34` | **0.34** | — | matches egui |
| `egui-wgpu` | `0.34` | **0.34** | — | **fixed** (was mispinned at 0.31 vs egui 0.27) |
| `egui_dock` | `0.19` | **0.19.1** (Mar 2026) | **1.92** | bumped 0.12 → 0.19; **drove toolchain floor** |
| `cosmic-text` | `0.19` | **0.19.0** (Apr 2026) | — | bumped 0.12 → 0.19 (Buffer methods drop `&mut FontSystem` arg) |

### Window / input

| Crate | Pinned | Latest stable | MSRV | Notes |
|---|---|---|---|---|
| `winit` | `0.30` (→ 0.30.13) | **0.30.13** (Mar 2026) | — | latest stable; 0.31 still beta-only |
| `gilrs` | `0.11` | **0.11.1** (Jan 2026) | — | bumped 0.10 → 0.11 |

### Physics / audio

| Crate | Pinned | Latest stable | MSRV | Notes |
|---|---|---|---|---|
| `rapier3d` | `0.32` | **0.32.0** (Jan 2026) | — | bumped 0.22 → 0.32 (10 minors); nalgebra → glam math types |
| `kira` | `0.12` | **0.12.0** (Feb 2026) | — | bumped 0.9 → 0.12; major restructure (modules, listener model, Volume → Decibels) |

### Asset formats

| Crate | Pinned | Latest stable | MSRV | Notes |
|---|---|---|---|---|
| `gltf` | `1.4` (→ 1.4.1) | **1.4.1** (May 2024) | — | `default-features = false` (per §1.6.4 one-importer-per-format — drops transitive `image`) |
| `image` | `0.25` (→ 0.25.10) | **0.25.10** (Mar 2026) | — | `default-features = false`; 0.25.7 yanked |
| `exr` | `1.74` | **1.74.0** (Nov 2025) | — | bumped 1.73 → 1.74 |

### Diagnostics

| Crate | Pinned | Latest stable | MSRV | Notes |
|---|---|---|---|---|
| `miette` | `7` (→ 7.6.0) | **7.6.0** (Apr 2025) | 1.70 | latest |

### Proc-macros

| Crate | Pinned | Latest stable | MSRV | Notes |
|---|---|---|---|---|
| `proc-macro2` | `1` (→ 1.0.106) | **1.0.106** (Jan 2026) | 1.68 | latest |
| `quote` | `1` (→ 1.0.45) | **1.0.45** (Mar 2026) | 1.71 | latest |
| `syn` | `2` (→ 2.0.117) | **2.0.117** (Feb 2026) | 1.71 | features: full, extra-traits |

### Crypto

| Crate | Pinned | Latest stable | MSRV | Notes |
|---|---|---|---|---|
| `ed25519-dalek` | `2.2` | **2.2.0** | — | bumped 2 → 2.2.0; 3.0 is pre-release. Phase 5 marketplace signing. |

---

## Crate-local dependencies (per-crate `[dependencies]`)

These are not in workspace `[workspace.dependencies]` (used by 1 crate only):

| Crate using | Dep | Pinned | Latest stable | Notes |
|---|---|---|---|---|
| `crates/audio` | `mint` | `0.5` (→ 0.5.9) | **0.5.9** (Feb 2022) | Math interop types; used by Kira spatial API |
| `crates/expr-wasm` | `criterion` | `0.5` | 0.8.2 (Feb 2026) | dev-dep; behind latest 3 minors. Bump candidate. |
| `crates/gfx` | `pollster` | `0.4` | **0.4.x** | Synchronous wgpu init (block_on); Phase 6.1 |
| `crates/gfx` | `bytemuck` | `1` (with `derive`) | latest 1.x | POD vertex structs; safe alternative to unsafe casts (Phase 6.1) |
| `crates/gfx` | `glam` | `0.30` (with `bytemuck`) | **0.30.x** (Apr 2026) | mat4/vec3 for Transform UBO (Phase 6.1 mesh-rendering); kept crate-local since gfx is the only direct consumer; promote to workspace.dependencies if anim/physics/cad-core take direct deps |
| `crates/io-gltf` | (workspace `gltf`) | — | — | uses workspace dep |
| `crates/pak-format` | `byteorder` | `1.5` (→ 1.5.0) | **1.5.0** (Oct 2023) | Endian-correct binary I/O |
| `crates/pak-format` | `memmap2` | `0.9` (→ 0.9.10) | **0.9.10** (Feb 2026) | Mmap'd reader for `.rge-pak` |
| `crates/pak-format` | `tempfile` | `3` (→ 3.27.0) | **3.27.0** (Mar 2026) | dev-dep |
| `crates/script-bench` | `criterion` | `0.5` | 0.8.2 | dev-dep; bump candidate |
| `crates/asset-store` | `tempfile` | `3` | **3.27.0** | dev-dep |
| `crates/editor-ui` | `tempfile` | `3` | **3.27.0** | dev-dep |
| `crates/ui-icons` | `tiny-skia` | `0.11` | 0.12.0 (Feb 2026) | SVG rasterizer; bump candidate (1 minor behind) |
| `crates/ui-theme` | `tempfile` | `3` | **3.27.0** | dev-dep |
| `crates/runtime-wasmtime-engine` | `wat` | `1` | (latest in 1.x train) | dev-dep; .wat → .wasm for tests |

---

## Transitive deps of note (Cargo.lock)

These appear multiple times in the lockfile — ecosystem version split, not direct pins. **Not blocking; informational.**

| Crate | Versions in lockfile | Source |
|---|---|---|
| `glam` | **18** versions: 0.14.0 → 0.32.1 | rapier3d/wgpu/egui/kira/cosmic-text each pin different minors |
| `windows-sys` | 5 | wasmtime/notify/etc. don't agree on a common version |
| `wasmparser` | 4 (0.244–0.248) | wasm-tools transitive split |
| `wasm-encoder` | 4 (0.244–0.248) | same |
| `wit-bindgen` | 2 (0.51 + 0.57) | 0.51 is wasmtime 44 internal; 0.57 is our explicit pin |
| `windows_*` per-arch shims | 3 each | normal Windows-target plumbing |
| `hashbrown` | 3 | std vs no_std vs older features |
| `getrandom` | 3 | rand transitive churn |
| `redox_syscall` | 3 | filesystem dep churn |
| `wit-parser` | 2 | wasm-tools |
| `wit-component` | 2 | wasm-tools |

**Aggregate package count:** 631 unique packages in `Cargo.lock` (up from 537 pre-bump; newer wasmtime + wgpu cones contribute more transitives).

---

## Bump candidates (currently behind latest)

| Crate | Pinned | Latest | Reason held |
|---|---|---|---|
| `criterion` | 0.5 | 0.8.2 | benchmark format compat; 3-minor jump deferred to next sweep |
| `tiny-skia` | 0.11 | 0.12.0 | held; W06 ui-icons stable on 0.11 — bump opportunity |
| `winit` | 0.30 | 0.31-beta | 0.31 still beta-only; held until stable |
| `ed25519-dalek` | 2.2 | 3.0-pre | held; 3.0 is pre-release |
| `notify` | 8 | 9.0-rc | held; 9.0 still release-candidate |

---

## Locked / pinned-down notes

The toolchain bump (1.78 → 1.92) **eliminated 9+ wave-internal `cargo update --precise` workarounds** from W01–W20 dispatch:

| Workaround | Pre-bump | Status now |
|---|---|---|
| `blake3 1.5.5` (W14, W10, W16 crate-local) | needed for cpufeatures 0.3 / edition2024 | obsolete; using workspace 1.8 |
| `uuid 1.10.0` (W13) | needed for rustc 1.85 MSRV | obsolete |
| `rangemap 1.5.1` (W07) | needed for rustc 1.81 (cosmic-text transitive) | obsolete |
| `unicode-segmentation 1.12.0` (W03) | winit 0.30.13 transitive | obsolete |
| `half 2.4.1`, `rayon 1.10`, `rayon-core 1.12.1` (W18) | image transitives | obsolete |
| `indexmap 2.7`, `spade 2.12` (W11) | rapier3d transitives | obsolete |
| `wat 1.220`, `spdx 0.10.8` (W04) | wasmtime transitives | obsolete |
| `clap =4.5.4` (W19) | criterion transitive needs edition2024 | obsolete |
| `ed25519-dalek =2.1.1` (W15 pak-format) | 2.2 raised MSRV to 1.81 | obsolete |
| `tempfile =3.20.0` (W16) | getrandom 0.3/0.4 edition2024 | obsolete |
| `cpufeatures = 0.2`, `base64ct < 1.7` (W15) | edition2024 forcing | obsolete |

---

## Re-audit cadence

Run `versions.md` re-audit at every minor version bump (per `plans/PLAN.md` §0.5). Tooling target: `tools/dependency-auditor` (currently stub) should:
1. Cross-check Cargo.toml pins vs crates.io latest
2. Flag MSRV drift past workspace toolchain
3. Detect lockfile duplicates beyond a threshold (e.g. >5 versions of same crate)
4. Output Markdown table compatible with this file

---

## See also

- [`Cargo.toml`](./Cargo.toml) — the pinned source of truth
- [`Cargo.lock`](./Cargo.lock) — full transitive resolution (631 packages)
- [`rust-toolchain.toml`](./rust-toolchain.toml) — channel + components + targets
- [`change.log`](./change.log) — running record including all version bumps
- [`Status.md`](./Status.md) — current workspace state + immediate next job
