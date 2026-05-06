//! `.rge-pak` writer — collects assets, sorts deterministically,
//! zstd-compresses, emits the file in `header → index → blobs →
//! signature` order.
//!
//! # Determinism contract (PLAN.md §1.6.10, §13.4)
//!
//! Two writes of identical assets in identical order MUST produce
//! a byte-identical pak. This module implements that guarantee:
//!
//! 1. Sort entries by `AssetId` (lexicographic on raw bytes —
//!    architectural-endianness-independent).
//! 2. zstd at fixed [`ZSTD_LEVEL`], single-threaded; the zstd lib
//!    is bit-deterministic for a given version+level+input on a
//!    single thread.
//! 3. Reserved bytes zero-filled.
//! 4. No timestamps, no PIDs, no random IVs, no multithreaded
//!    interleaving.
//! 5. Signature region zero-filled when no signing key is supplied
//!    (so unsigned paks remain byte-identical across cooks).
//!
//! Verified by `tests/determinism_gate.rs` (CI gate).

use std::io::Write;

use crate::header::{CompressionAlgo, PakHeader, HEADER_SIZE};
use crate::index::{AssetId, AssetKind, IndexEntry, IndexTable, INDEX_ENTRY_SIZE};
use crate::signature::SIGNATURE_SIZE;
use crate::PakError;

/// zstd compression level used for blob payloads. Level 3 is the
/// zstd default — a balance between cook time and decompression
/// speed at runtime. Higher levels (e.g. 19) yield smaller paks but
/// 30x slower cooks; the cook budget is more constrained than the
/// distribution-size budget, so 3 is the v1 default.
///
/// **This is part of the determinism contract: changing the level
/// changes the wire bytes.** A change here requires bumping
/// [`crate::header::ENGINE_VERSION`].
pub const ZSTD_LEVEL: i32 = 3;

/// Pending asset, queued before [`PakWriter::finish`]. Holds the
/// uncompressed payload + metadata needed to construct the index
/// entry once layout is known.
struct StagedAsset {
    asset_id: AssetId,
    kind: AssetKind,
    uncompressed: Vec<u8>,
}

/// Builder for an `.rge-pak` payload.
///
/// ```ignore
/// use rge_pak_format::{PakWriter, AssetKind};
/// let mut w = PakWriter::new();
/// w.add_asset_auto_id(AssetKind::Mesh, b"<mesh blob>".to_vec());
/// let bytes = w.finish().unwrap();
/// std::fs::write("desktop.rge-pak", bytes).unwrap();
/// ```
pub struct PakWriter {
    staged: Vec<StagedAsset>,
    compression_algo: CompressionAlgo,
}

impl Default for PakWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl PakWriter {
    /// Construct an empty writer using the default codec (zstd).
    #[must_use]
    pub fn new() -> Self {
        Self {
            staged: Vec::new(),
            compression_algo: CompressionAlgo::Zstd,
        }
    }

    /// Construct an empty writer with an explicit codec. Used by
    /// tests and the small-pak fast path; production cookers should
    /// stick with [`Self::new`].
    #[must_use]
    pub fn with_compression(algo: CompressionAlgo) -> Self {
        Self {
            staged: Vec::new(),
            compression_algo: algo,
        }
    }

    /// Stage an asset with an explicit [`AssetId`]. Use this when
    /// the id was computed earlier in the cook pipeline (e.g. by
    /// the asset-store's content-addressing).
    ///
    /// Duplicate ids are collapsed last-wins by [`IndexTable::from_unsorted`].
    pub fn add_asset(&mut self, asset_id: AssetId, kind: AssetKind, bytes: Vec<u8>) {
        self.staged.push(StagedAsset {
            asset_id,
            kind,
            uncompressed: bytes,
        });
    }

    /// Stage an asset, computing the id from the content. Convenience
    /// for callers that don't already have a hash.
    pub fn add_asset_auto_id(&mut self, kind: AssetKind, bytes: Vec<u8>) -> AssetId {
        let id = AssetId::from_bytes(&bytes);
        self.add_asset(id, kind, bytes);
        id
    }

    /// Number of staged assets.
    #[must_use]
    pub fn len(&self) -> usize {
        self.staged.len()
    }

    /// True when nothing has been staged.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.staged.is_empty()
    }

    /// Compress, lay out, and emit the full pak as a `Vec<u8>`.
    ///
    /// Layout: `header → index → blobs → signature region`.
    ///
    /// # Errors
    ///
    /// Returns [`PakError::Zstd`] on compression failure or
    /// [`PakError::Io`] on the (vanishingly unlikely) write-to-Vec
    /// failure.
    ///
    /// # Panics
    ///
    /// Panics if the cumulative blob size overflows `u64` — i.e.
    /// the pak exceeds 16 EiB. This is unreachable on any
    /// realistic hardware; the panic is a defensive guard against
    /// integer-wrap silent corruption rather than a real failure
    /// mode callers need to handle.
    #[allow(clippy::cast_possible_truncation)] // pak size fits in usize on 64-bit; 32-bit hosts can't open a 4GiB+ pak anyway
    pub fn finish(self) -> Result<Vec<u8>, PakError> {
        // Step 1: compress every blob up-front so we know its
        // compressed size — which the index needs, and which feeds
        // the file-offset arithmetic.
        let mut compressed_blobs: Vec<(AssetId, AssetKind, Vec<u8>, u64)> =
            Vec::with_capacity(self.staged.len());
        for staged in self.staged {
            let uncompressed_len = staged.uncompressed.len() as u64;
            let compressed = match self.compression_algo {
                CompressionAlgo::None => staged.uncompressed,
                CompressionAlgo::Zstd => zstd::bulk::compress(&staged.uncompressed, ZSTD_LEVEL)
                    .map_err(PakError::zstd)?,
            };
            compressed_blobs.push((staged.asset_id, staged.kind, compressed, uncompressed_len));
        }

        // Step 2: sort by asset_id. THIS IS THE DETERMINISM POINT.
        // `sort_by` is stable; ties (same asset_id from two sources)
        // are broken by input order, then collapsed last-wins by
        // `IndexTable::from_unsorted` below.
        compressed_blobs.sort_by(|a, b| a.0.cmp(&b.0));

        // Step 3: compute layout offsets. The index region's wire
        // size depends on the post-dedup entry count, but we
        // deduplicate AFTER sorting so the count may differ from
        // `compressed_blobs.len()`. To keep the math simple we run
        // the dedup pass here.
        let entries_pre_dedup = compressed_blobs.len();
        compressed_blobs = dedup_keep_last_by_id(compressed_blobs);
        let final_entry_count = compressed_blobs.len();
        debug_assert!(final_entry_count <= entries_pre_dedup);

        let index_wire_size = 8 + final_entry_count * INDEX_ENTRY_SIZE;
        let blobs_start = HEADER_SIZE + index_wire_size;

        // Step 4: build the IndexTable. Offsets reference the
        // (final, post-dedup) blob arena.
        let mut entries = Vec::with_capacity(final_entry_count);
        let mut blob_cursor: u64 = blobs_start as u64;
        for (asset_id, kind, blob, uncompressed_len) in &compressed_blobs {
            entries.push(IndexEntry {
                asset_id: *asset_id,
                offset: blob_cursor,
                compressed_length: blob.len() as u64,
                uncompressed_length: *uncompressed_len,
                kind: *kind,
                flags: 0,
            });
            blob_cursor = blob_cursor
                .checked_add(blob.len() as u64)
                .expect("pak overflow: total blob size exceeds u64");
        }
        let index_table = IndexTable::from_unsorted(entries);
        // Sanity: post-dedup index size must match what we reserved.
        debug_assert_eq!(index_table.len(), final_entry_count);

        // Step 5: serialise. Pre-allocate the full output vec;
        // every byte of size is now known, so this is one alloc.
        let total_size = blob_cursor as usize + SIGNATURE_SIZE;
        let mut out = Vec::with_capacity(total_size);

        // Header.
        let header = PakHeader::current(self.compression_algo);
        header.write_into(&mut out)?;
        debug_assert_eq!(out.len(), HEADER_SIZE);

        // Index.
        index_table.write_into(&mut out)?;
        debug_assert_eq!(out.len(), HEADER_SIZE + index_wire_size);

        // Blobs.
        for (_, _, blob, _) in &compressed_blobs {
            out.write_all(blob)?;
        }
        debug_assert_eq!(out.len(), blob_cursor as usize);

        // Signature region (Phase-5 stub; zero-filled when unsigned).
        // Zero fill here guarantees byte-identical output for
        // identical inputs — see signature.rs for the Phase-5 plan.
        out.extend_from_slice(&[0u8; SIGNATURE_SIZE]);
        debug_assert_eq!(out.len(), total_size);

        Ok(out)
    }
}

/// Dedup post-sort, keeping the LAST entry per `asset_id`. Mirror of
/// [`IndexTable::from_unsorted`]'s dedup, but operates on the
/// `(id, kind, blob, ulen)` tuple list so we don't duplicate work.
fn dedup_keep_last_by_id(
    mut v: Vec<(AssetId, AssetKind, Vec<u8>, u64)>,
) -> Vec<(AssetId, AssetKind, Vec<u8>, u64)> {
    // After sort_by, equal ids are adjacent. For each run we want
    // to keep the LAST element. `Vec::dedup_by` keeps the first, so
    // reverse-dedup-reverse like in `index.rs`.
    if v.windows(2).any(|w| w[0].0 == w[1].0) {
        v.reverse();
        v.dedup_by(|a, b| a.0 == b.0);
        v.reverse();
    }
    v
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::reader::PakReader;

    #[test]
    fn empty_writer_emits_header_and_signature_only() {
        let bytes = PakWriter::new().finish().unwrap();
        // Empty pak = HEADER_SIZE + 8 (entry_count=0) + 0 blobs + SIGNATURE_SIZE.
        assert_eq!(bytes.len(), HEADER_SIZE + 8 + SIGNATURE_SIZE);
        // Signature region is zero (unsigned).
        assert!(bytes[bytes.len() - SIGNATURE_SIZE..]
            .iter()
            .all(|&b| b == 0));
    }

    #[test]
    fn finish_then_reader_round_trips_blob() {
        let mut w = PakWriter::new();
        let payload = b"the quick brown fox jumps over the lazy dog".to_vec();
        let id = w.add_asset_auto_id(AssetKind::Mesh, payload.clone());
        let bytes = w.finish().unwrap();

        let reader = PakReader::open_bytes(bytes).unwrap();
        let recovered = reader.lookup(&id).unwrap().unwrap();
        assert_eq!(recovered.as_ref(), payload.as_slice());
    }

    #[test]
    fn sort_is_deterministic() {
        // Same inputs in different staged order → identical pak.
        let make_pak = |order: &[u8]| {
            let mut w = PakWriter::new();
            for &n in order {
                let blob = vec![n; 16];
                w.add_asset_auto_id(AssetKind::Opaque, blob);
            }
            w.finish().unwrap()
        };
        let a = make_pak(&[1, 2, 3, 4, 5]);
        let b = make_pak(&[5, 4, 3, 2, 1]);
        assert_eq!(a, b, "writer must be input-order-independent");
    }

    #[test]
    fn duplicate_ids_collapse_last_wins() {
        let mut w = PakWriter::new();
        let id = AssetId::from_bytes(b"shared");
        // Add the SAME id twice; index should collapse to one
        // entry. We use explicit add_asset to bypass auto-hashing
        // so the two payloads can differ.
        w.add_asset(id, AssetKind::Opaque, b"first".to_vec());
        w.add_asset(id, AssetKind::Opaque, b"second".to_vec());
        let bytes = w.finish().unwrap();
        let reader = PakReader::open_bytes(bytes).unwrap();
        // Last-wins: should observe "second".
        let recovered = reader.lookup(&id).unwrap().unwrap();
        assert_eq!(recovered.as_ref(), b"second");
    }

    #[test]
    fn uncompressed_codec_round_trips() {
        let mut w = PakWriter::with_compression(CompressionAlgo::None);
        let id = w.add_asset_auto_id(AssetKind::Opaque, b"raw payload".to_vec());
        let bytes = w.finish().unwrap();
        let reader = PakReader::open_bytes(bytes).unwrap();
        let recovered = reader.lookup(&id).unwrap().unwrap();
        assert_eq!(recovered.as_ref(), b"raw payload");
    }
}
