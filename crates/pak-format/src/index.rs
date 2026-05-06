//! Sorted asset index — `IndexEntry` records and binary-search lookup.
//!
//! Spec: W15 dispatch package §2 (sorted index, O(log n) lookup) +
//! PLAN.md §1.6.3 (AssetId = `blake3:<hash>`).
//!
//! # Wire layout
//!
//! ```text
//! [u64 LE]   entry_count
//! [N * IndexEntry]   sorted strictly ascending by asset_id
//! ```
//!
//! Each entry is exactly [`INDEX_ENTRY_SIZE`] (60) bytes:
//!
//! ```text
//! offset  size  field
//! ------  ----  --------------------------------
//! 0x00    32    asset_id (blake3 raw digest)
//! 0x20    8     offset (u64 LE)              absolute file offset of blob
//! 0x28    8     compressed_length (u64 LE)
//! 0x30    8     uncompressed_length (u64 LE) for decompression buffer pre-size
//! 0x38    2     kind (u16 LE enum)
//! 0x3A    2     flags (u16 LE bitfield)
//! ------  ----  -------------------------------
//! total = 60 bytes
//! ```
//!
//! # AssetId encoding
//!
//! PLAN.md §1.6.3 defines the user-facing `AssetId` as the string
//! `blake3:<hex-digest>`. On the wire we store the raw 32-byte
//! digest only — the `blake3:` prefix is reconstructed by
//! `AssetId::to_string()`. This saves 8 bytes per entry × 10k entries
//! = 80 KB of header bloat, and the prefix carries no information
//! the file format itself doesn't already imply.

use std::io::{Read, Write};

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

use crate::PakError;

/// Wire-size of one index entry. **Do not change without bumping
/// `header::ENGINE_VERSION`.**
pub const INDEX_ENTRY_SIZE: usize = 60;

/// Length of a blake3 digest in bytes.
const BLAKE3_DIGEST_LEN: usize = 32;

/// Hard cap on the index region size, to keep a maliciously crafted
/// pak header from triggering an OOM at parse time. 64 MiB of index
/// entries = ~1.1 M assets; well above any real cook size and below
/// any host's RAM limit.
const MAX_INDEX_ENTRIES: u64 = (64 * 1024 * 1024) / INDEX_ENTRY_SIZE as u64;

/// Content-addressed asset identifier.
///
/// Re-export of the canonical [`rge_kernel_asset::AssetId`] (Phase 4.1).
/// The user-visible form per PLAN.md §1.6.3 is `"blake3:<hex>"` — use
/// `AssetId::to_string()` / `AssetId::from_bytes()` / `AssetId::from_raw()`.
pub use rge_kernel_asset::AssetId;

/// Asset kind. Stored on the wire as `u16 LE` in each [`IndexEntry`].
///
/// The enum values are stable across engine versions; appending a
/// new kind is a non-breaking change. **Do not reorder existing
/// values.**
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum AssetKind {
    /// Generic opaque blob (test fixture, fallback).
    Opaque = 0,
    /// Triangle mesh (vertices + indices). Cooked from glTF/STEP.
    Mesh = 1,
    /// 2D texture. Cooked from PNG/JPEG.
    Texture = 2,
    /// PCM audio. Cooked from WAV.
    Audio = 3,
    /// Material (PBR, parameters baked).
    Material = 4,
    /// Animation clip.
    AnimClip = 5,
    /// Pre-compiled shader (WGSL / SPIR-V / DXIL).
    Shader = 6,
    /// AOT-compiled WASM script (`.cwasm`).
    Script = 7,
    /// Scene graph dump.
    Scene = 8,
    /// Prefab template.
    Prefab = 9,
    /// Future use; the writer rejects unknown kinds at compile time
    /// because it takes the enum, but the reader sees them as
    /// [`AssetKind::Unknown`] for forward-compat.
    Unknown = 0xFFFF,
}

impl AssetKind {
    /// Convert from a wire u16. Unknown values map to
    /// [`AssetKind::Unknown`] (forward-compat: a newer cooker may
    /// have introduced a kind this reader does not know).
    #[must_use]
    pub fn from_u16(v: u16) -> Self {
        match v {
            0 => Self::Opaque,
            1 => Self::Mesh,
            2 => Self::Texture,
            3 => Self::Audio,
            4 => Self::Material,
            5 => Self::AnimClip,
            6 => Self::Shader,
            7 => Self::Script,
            8 => Self::Scene,
            9 => Self::Prefab,
            _ => Self::Unknown,
        }
    }

    /// Inverse of [`from_u16`].
    #[must_use]
    pub fn to_u16(self) -> u16 {
        self as u16
    }
}

/// One row of the sorted index.
///
/// The writer collects entries unsorted, then `sort()`-s by
/// `asset_id` before serialising; the reader checks the index is
/// sorted at open time. Sortedness is the precondition for binary
/// search (see [`IndexTable::lookup`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexEntry {
    /// Content hash of the (uncompressed) blob.
    pub asset_id: AssetId,
    /// Absolute file offset of the (compressed) blob.
    pub offset: u64,
    /// Length of the compressed blob in the file.
    pub compressed_length: u64,
    /// Length of the uncompressed blob — used to size the
    /// decompression buffer at lookup time.
    pub uncompressed_length: u64,
    /// What this asset is (see [`AssetKind`]).
    pub kind: AssetKind,
    /// Per-blob flags. Currently zero. Future use: e.g. "blob is
    /// stored uncompressed even when header says zstd" for tiny
    /// blobs where compression hurts.
    pub flags: u16,
}

impl IndexEntry {
    /// Serialise into 60 bytes.
    pub fn write_into<W: Write>(&self, w: &mut W) -> Result<(), PakError> {
        w.write_all(self.asset_id.raw())?;
        w.write_u64::<LittleEndian>(self.offset)?;
        w.write_u64::<LittleEndian>(self.compressed_length)?;
        w.write_u64::<LittleEndian>(self.uncompressed_length)?;
        w.write_u16::<LittleEndian>(self.kind.to_u16())?;
        w.write_u16::<LittleEndian>(self.flags)?;
        Ok(())
    }

    /// Deserialise 60 bytes into an entry.
    pub fn read_from<R: Read>(r: &mut R) -> Result<Self, PakError> {
        let mut digest = [0u8; BLAKE3_DIGEST_LEN];
        r.read_exact(&mut digest)?;
        let offset = r.read_u64::<LittleEndian>()?;
        let compressed_length = r.read_u64::<LittleEndian>()?;
        let uncompressed_length = r.read_u64::<LittleEndian>()?;
        let kind_raw = r.read_u16::<LittleEndian>()?;
        let flags = r.read_u16::<LittleEndian>()?;
        Ok(Self {
            asset_id: AssetId::from_raw(digest),
            offset,
            compressed_length,
            uncompressed_length,
            kind: AssetKind::from_u16(kind_raw),
            flags,
        })
    }
}

/// In-memory view of the index region. Owns its entries; the
/// writer builds one from a flat `Vec<IndexEntry>`, the reader
/// materialises one by reading the count + entries from the file.
///
/// Invariant maintained on construction: entries are sorted
/// strictly ascending by `asset_id`. The writer enforces this
/// via [`IndexTable::from_unsorted`]; the reader enforces it via
/// [`IndexTable::validate_sorted`] after reading.
#[derive(Debug, Clone)]
pub struct IndexTable {
    entries: Vec<IndexEntry>,
}

impl IndexTable {
    /// Sort an unsorted list of entries and return the table. Last
    /// entry wins on duplicate asset_ids — duplicate content has
    /// the same hash anyway, so this only matters if the caller
    /// genuinely staged the same id twice; in that case "last wins"
    /// matches typical asset-pipeline override semantics.
    ///
    /// Sorts using `sort_by` (stable) so that for entries with
    /// equal ids the input order is preserved before the dedup
    /// step picks the trailing element.
    #[must_use]
    pub fn from_unsorted(mut entries: Vec<IndexEntry>) -> Self {
        entries.sort_by(|a, b| a.asset_id.cmp(&b.asset_id));
        // Dedup keeping the LAST occurrence per id. `Vec::dedup_by`
        // keeps the FIRST, so we walk in reverse, dedup, then
        // reverse back. This is O(n) extra work on top of the sort
        // but only on the duplicate path; in the common case (no
        // duplicates) it's a no-op single pass.
        if Self::has_duplicate_ids(&entries) {
            entries.reverse();
            entries.dedup_by(|a, b| a.asset_id == b.asset_id);
            entries.reverse();
        }
        Self { entries }
    }

    fn has_duplicate_ids(entries: &[IndexEntry]) -> bool {
        entries.windows(2).any(|w| w[0].asset_id == w[1].asset_id)
    }

    /// Borrow the entries.
    #[must_use]
    pub fn entries(&self) -> &[IndexEntry] {
        &self.entries
    }

    /// Number of entries (after dedup).
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when the index is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Total wire-size of the index region in bytes:
    /// `8 (entry_count) + N * INDEX_ENTRY_SIZE`.
    #[must_use]
    pub fn wire_size(&self) -> usize {
        8 + self.entries.len() * INDEX_ENTRY_SIZE
    }

    /// O(log n) lookup by asset id. Returns the entry if present.
    #[must_use]
    pub fn lookup(&self, id: &AssetId) -> Option<&IndexEntry> {
        match self.entries.binary_search_by(|e| e.asset_id.cmp(id)) {
            Ok(idx) => Some(&self.entries[idx]),
            Err(_) => None,
        }
    }

    /// Serialise: `u64 entry_count` followed by entries. Caller
    /// guarantees `w` is positioned at the index region.
    pub fn write_into<W: Write>(&self, w: &mut W) -> Result<(), PakError> {
        // Wire-cast to u64; on 32-bit platforms `usize::MAX < u64::MAX`,
        // which is correct (an index larger than 4 G entries cannot
        // exist in any real cook output).
        w.write_u64::<LittleEndian>(self.entries.len() as u64)?;
        for entry in &self.entries {
            entry.write_into(w)?;
        }
        Ok(())
    }

    /// Deserialise: reads `u64 entry_count` then that many entries.
    /// Validates the sortedness invariant before returning.
    pub fn read_from<R: Read>(r: &mut R) -> Result<Self, PakError> {
        let entry_count = r.read_u64::<LittleEndian>()?;
        // Sanity-bound (see [`MAX_INDEX_ENTRIES`]).
        if entry_count > MAX_INDEX_ENTRIES {
            return Err(PakError::IndexOutOfBounds {
                entry_count,
                entry_size: INDEX_ENTRY_SIZE,
                needed: (entry_count as usize).saturating_mul(INDEX_ENTRY_SIZE),
                available: 0,
            });
        }
        let mut entries = Vec::with_capacity(entry_count as usize);
        for _ in 0..entry_count {
            entries.push(IndexEntry::read_from(r)?);
        }
        let table = Self { entries };
        table.validate_sorted()?;
        Ok(table)
    }

    /// Verify the strictly-ascending invariant. Called after read.
    pub fn validate_sorted(&self) -> Result<(), PakError> {
        for (i, w) in self.entries.windows(2).enumerate() {
            if w[0].asset_id >= w[1].asset_id {
                return Err(PakError::IndexNotSorted { index: i + 1 });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn id_n(n: u8) -> AssetId {
        let mut d = [0u8; 32];
        d[0] = n;
        AssetId::from_raw(d)
    }

    fn entry_n(n: u8) -> IndexEntry {
        IndexEntry {
            asset_id: id_n(n),
            offset: u64::from(n) * 1000,
            compressed_length: 100,
            uncompressed_length: 200,
            kind: AssetKind::Opaque,
            flags: 0,
        }
    }

    #[test]
    fn entry_size_constant_is_sixty() {
        assert_eq!(INDEX_ENTRY_SIZE, 60);
        let e = entry_n(0);
        let mut buf = Vec::new();
        e.write_into(&mut buf).unwrap();
        assert_eq!(buf.len(), INDEX_ENTRY_SIZE);
    }

    #[test]
    fn entry_round_trips() {
        let e = IndexEntry {
            asset_id: AssetId::from_bytes(b"hello"),
            offset: 1234,
            compressed_length: 500,
            uncompressed_length: 1024,
            kind: AssetKind::Mesh,
            flags: 0xABCD,
        };
        let mut buf = Vec::new();
        e.write_into(&mut buf).unwrap();
        let parsed = IndexEntry::read_from(&mut std::io::Cursor::new(&buf)).unwrap();
        assert_eq!(parsed, e);
    }

    #[test]
    fn from_unsorted_sorts_ascending() {
        let table = IndexTable::from_unsorted(vec![entry_n(3), entry_n(1), entry_n(2)]);
        assert_eq!(table.entries()[0].asset_id, id_n(1));
        assert_eq!(table.entries()[1].asset_id, id_n(2));
        assert_eq!(table.entries()[2].asset_id, id_n(3));
    }

    #[test]
    fn duplicate_ids_collapse_last_wins() {
        let mut e1 = entry_n(5);
        e1.offset = 100;
        let mut e2 = entry_n(5);
        e2.offset = 999; // last one for this id
        let table = IndexTable::from_unsorted(vec![e1, e2]);
        assert_eq!(table.len(), 1);
        assert_eq!(table.entries()[0].offset, 999);
    }

    #[test]
    fn lookup_is_log_n() {
        // We can't measure log-n directly in a unit test, but we can
        // verify correctness. The 10k-entry perf test lives in
        // `tests/round_trip.rs`.
        let table = IndexTable::from_unsorted((0u8..50).map(entry_n).collect());
        for i in 0..50 {
            let hit = table.lookup(&id_n(i)).unwrap();
            assert_eq!(hit.offset, u64::from(i) * 1000);
        }
        // Miss case.
        let miss = id_n(200);
        assert!(table.lookup(&miss).is_none());
    }

    #[test]
    fn validate_sorted_catches_unsorted() {
        // Build a table by-hand bypassing `from_unsorted` to stage
        // the invariant violation.
        let bad = IndexTable {
            entries: vec![entry_n(5), entry_n(3)],
        };
        let err = bad.validate_sorted().unwrap_err();
        match err {
            PakError::IndexNotSorted { index } => assert_eq!(index, 1),
            other => panic!("expected IndexNotSorted, got {other:?}"),
        }
    }

    #[test]
    fn validate_sorted_catches_duplicates() {
        let bad = IndexTable {
            entries: vec![entry_n(5), entry_n(5)],
        };
        // strict-ascending means equality is also a violation.
        assert!(bad.validate_sorted().is_err());
    }

    #[test]
    fn assetid_display_is_blake3_hex() {
        let id = AssetId::from_bytes(b"");
        let s = id.to_string();
        assert!(s.starts_with("blake3:"));
        assert_eq!(s.len(), 7 + 64);
        // blake3("") known constant prefix:
        assert!(s.starts_with("blake3:af1349b9f5"));
    }

    #[test]
    fn table_round_trips() {
        let entries: Vec<_> = (0u8..10).map(entry_n).collect();
        let table = IndexTable::from_unsorted(entries.clone());
        let mut buf = Vec::new();
        table.write_into(&mut buf).unwrap();
        // entry_count(8) + 10 * 60 = 608 bytes
        assert_eq!(buf.len(), 8 + 10 * INDEX_ENTRY_SIZE);
        let parsed = IndexTable::read_from(&mut std::io::Cursor::new(&buf)).unwrap();
        assert_eq!(parsed.entries(), table.entries());
    }
}
