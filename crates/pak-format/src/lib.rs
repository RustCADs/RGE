//! `rge-pak-format` — `.rge-pak` cooked-asset container.
//!
//! # Wire format (stable v1)
//!
//! A `.rge-pak` file is laid out as four contiguous regions:
//!
//! ```text
//! +-----------------------------+
//! | Header (32 bytes, fixed)    |  magic "RGEP" + versions + flags
//! +-----------------------------+
//! | Index region                |  u64 entry_count + entry_count * IndexEntry
//! +-----------------------------+
//! | Blob arena                  |  zstd-compressed blobs, contiguous
//! +-----------------------------+
//! | Optional Ed25519 signature  |  Phase-5 marketplace integrity (stub at v0.0.1)
//! +-----------------------------+
//! ```
//!
//! ## Header (32 bytes — see [`header`])
//!
//! ```text
//! offset  size  field
//! ------  ----  ---------------------------
//! 0x00    4     magic              b"RGEP"
//! 0x04    4     engine_version     u32 LE
//! 0x08    4     player_state_schema_version  u32 LE
//! 0x0C    4     flags              u32 LE bitfield
//! 0x10    1     compression algo   u8 enum (0=none, 1=zstd)
//! 0x11    15    reserved (zero)
//! ```
//!
//! ## Index region
//!
//! ```text
//! [u64 LE]   entry_count
//! [N * IndexEntry]   sorted strictly ascending by asset_id
//! ```
//!
//! Each [`IndexEntry`] is a fixed 60 bytes (see [`index`]).
//!
//! ## Blob arena
//!
//! Each blob is referenced by `(offset, length)` in its [`IndexEntry`]; the
//! offset is absolute from the start of the file. Blobs are zstd-compressed
//! when `compression_algo == ZSTD`; the index also records the uncompressed
//! length so the reader can pre-size the decompression buffer.
//!
//! # Determinism guarantees (per PLAN.md §1.6.10)
//!
//! Two writes of identical assets in identical order produce a byte-identical
//! `.rge-pak`. To preserve this:
//!
//! 1. [`writer::PakWriter::add_asset`] sorts entries by `AssetId` (stable;
//!    duplicates collapse to last-wins) before serialisation.
//! 2. zstd is invoked at a fixed compression level with no internal
//!    threading (single-threaded zstd is bit-deterministic for a given
//!    library version).
//! 3. Reserved bytes are zero-initialised; no uninitialised padding leaks.
//! 4. The optional signature is computed over header+index hash only;
//!    when the signing key is absent, the trailing 64 zero bytes preserve
//!    file size but the signature region is zeroed (still byte-identical).
//!
//! # Adapted from rustforge prior art
//!
//! `// adapted from rustforge::crates::manufacturing-format-ctb on 2026-05-05
//! // — header+index pattern for .rge-pak`
//!
//! Specifically: fixed-layout little-endian header with a magic+version
//! prefix, hand-rolled with `byteorder` (no `bincode`/`serde_binary`
//! length prefixes that would silently change the layout). Per PLAN.md
//! Rule 2 (steal-and-adapt) and the W15 dispatch package §3.
//!
//! # Local relax of `unsafe_code = forbid`
//!
//! The reader uses `memmap2::Mmap::map`, which is `unsafe` because
//! mutating the mapped file from another process while we hold the
//! map is UB. For a read-only cooked pak the same risk window
//! exists for ordinary `File::read`; we accept it explicitly and
//! document the precondition at the call site (`reader.rs`).
//!
//! This is the ONLY `unsafe` in the crate.

#![allow(clippy::module_name_repetitions)]
// Pedantic-class lints we accept project-wide:
//
// - `cast_possible_truncation` on u64→usize is fine on the 64-bit
//   targets in `rust-toolchain.toml`; pak files don't fit on 32-bit
//   hosts anyway. Per-call `try_from`s would clutter the I/O paths
//   without buying anything.
// - `missing_errors_doc` on every `Result`-returning helper is
//   covered by the single error type [`PakError`]; rather than
//   inline-document each variant per function we point readers at
//   the type's own doc.
// - `doc_markdown` for `asset_id` etc. — these read better in
//   prose than backticked.
#![allow(
    clippy::cast_possible_truncation,
    clippy::missing_errors_doc,
    clippy::doc_markdown
)]
// Forbidden at workspace level (`unsafe_code = "forbid"`); relaxed
// to `deny` for this single mmap call, justified above. Any new
// `unsafe` in this crate requires a fresh review.
#![deny(unsafe_code)]

mod errors;
pub mod header;
pub mod index;
pub mod reader;
pub mod signature;
pub mod writer;

pub use errors::PakError;
pub use header::{
    CompressionAlgo, PakHeader, ENGINE_VERSION, HEADER_SIZE, MAGIC, PLAYER_STATE_SCHEMA_VERSION,
};
pub use index::{AssetId, AssetKind, IndexEntry, INDEX_ENTRY_SIZE};
pub use reader::PakReader;
pub use signature::{verify_signature, SIGNATURE_SIZE};
pub use writer::PakWriter;
