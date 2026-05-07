//! `.rge-pak` reader — mmap'd, lazy decompression on lookup.
//!
//! # Design
//!
//! Open: parse the header + index from a memory-mapped view of the
//! file. The blob arena stays mmap'd; lookup decompresses only the
//! requested blob. This is the §13.4 100MB-pak-loads-<500ms gate
//! point: opening a 100 MB pak does ~32 KB of header+index reads
//! plus a single mmap call (constant time vs file size).
//!
//! Two open paths:
//!
//! - [`PakReader::open`] — production. Maps the file via `memmap2`.
//! - [`PakReader::open_bytes`] — in-memory. Used by tests and by
//!   callers that already have the bytes (network fetch, embedded
//!   resource). Keeps the same lookup API.

use std::borrow::Cow;
use std::fs::File;
use std::io::Cursor;
use std::path::Path;

use memmap2::Mmap;

use crate::header::{CompressionAlgo, PakHeader, HEADER_SIZE};
use crate::index::{AssetId, AssetKind, IndexEntry, IndexTable};
use crate::PakError;

/// Backing storage for an open pak. Either mmap'd or owned bytes.
enum PakBacking {
    Mmap(Mmap),
    Owned(Vec<u8>),
}

impl PakBacking {
    fn as_slice(&self) -> &[u8] {
        match self {
            Self::Mmap(m) => m,
            Self::Owned(v) => v,
        }
    }
}

/// Opened, queryable `.rge-pak`.
///
/// Construct via [`PakReader::open`] (mmap) or [`PakReader::open_bytes`]
/// (owned). Then call [`PakReader::lookup`] for O(log n) random
/// access.
pub struct PakReader {
    backing: PakBacking,
    header: PakHeader,
    index: IndexTable,
}

impl std::fmt::Debug for PakReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Don't print backing bytes (could be megabytes); summarise.
        f.debug_struct("PakReader")
            .field("header", &self.header)
            .field("entry_count", &self.index.len())
            .field("backing_size", &self.backing.as_slice().len())
            .finish()
    }
}

/// Result of [`PakReader::lookup`]. Either a borrowed slice (when
/// the asset is stored uncompressed) or an owned decompressed
/// buffer. Most callers can treat both the same via `Cow::Borrowed`/
/// `Cow::Owned` ergonomics or just `as_ref()`.
pub type PakBlob<'a> = Cow<'a, [u8]>;

impl PakReader {
    /// Open a pak from a path. Maps the file read-only.
    ///
    /// # Errors
    ///
    /// Returns [`PakError::Io`] on file-open / mmap failure,
    /// [`PakError::BadMagic`] / [`PakError::Truncated`] / etc. on
    /// structural problems.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, PakError> {
        let file = File::open(path)?;
        // SAFETY: `Mmap::map` is `unsafe` because UB arises only if
        // another process truncates or writes to the underlying file
        // while we hold the map. For a read-only cooked pak (which
        // the asset-store treats as immutable once written) the same
        // risk window exists for ordinary `File::read` — we accept
        // it explicitly. This is the only `unsafe` in the crate;
        // the workspace `unsafe_code = forbid` lint is locally
        // relaxed to `deny` in this crate's `Cargo.toml`.
        #[allow(unsafe_code)]
        let mmap = unsafe { Mmap::map(&file)? };
        Self::from_backing(PakBacking::Mmap(mmap))
    }

    /// Open a pak from in-memory bytes. Used by tests and by
    /// callers that already have the file body (e.g. fetched over
    /// the network, embedded as `include_bytes!`).
    pub fn open_bytes(bytes: Vec<u8>) -> Result<Self, PakError> {
        Self::from_backing(PakBacking::Owned(bytes))
    }

    fn from_backing(backing: PakBacking) -> Result<Self, PakError> {
        let bytes = backing.as_slice();
        if bytes.len() < HEADER_SIZE {
            return Err(PakError::Truncated {
                got: bytes.len(),
                need: HEADER_SIZE,
            });
        }

        // Header.
        let mut cursor = Cursor::new(&bytes[..HEADER_SIZE]);
        let header = PakHeader::read_from(&mut cursor)?;

        // Index.
        let mut index_cursor = Cursor::new(&bytes[HEADER_SIZE..]);
        let index = IndexTable::read_from(&mut index_cursor)?;

        // Validate every blob extent fits in the file. This is the
        // single most important defence against a maliciously
        // crafted pak (e.g. a "zip slip"-style extent that tries
        // to read OOB memory). We do it eagerly at open because
        // the cost is O(n) over the index — already-paid by the
        // index parse — and avoids a per-lookup branch.
        let file_size = bytes.len() as u64;
        for entry in index.entries() {
            let end = entry
                .offset
                .checked_add(entry.compressed_length)
                .ok_or_else(|| PakError::BlobOutOfBounds {
                    asset_id: entry.asset_id.to_string(),
                    offset: entry.offset,
                    length: entry.compressed_length,
                    file_size,
                })?;
            if end > file_size {
                return Err(PakError::BlobOutOfBounds {
                    asset_id: entry.asset_id.to_string(),
                    offset: entry.offset,
                    length: entry.compressed_length,
                    file_size,
                });
            }
        }

        Ok(Self {
            backing,
            header,
            index,
        })
    }

    /// Borrow the parsed header. Useful for engine-version
    /// migration logic.
    #[must_use]
    pub fn header(&self) -> &PakHeader {
        &self.header
    }

    /// Borrow the parsed index. Most callers want [`Self::lookup`]
    /// instead, but the asset-store iterates the index for cook-
    /// content audit.
    #[must_use]
    pub fn index(&self) -> &IndexTable {
        &self.index
    }

    /// Number of assets in the pak.
    #[must_use]
    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// True when the pak contains no assets.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// O(log n) lookup by asset id. Returns:
    ///
    /// - `Ok(None)` if the id is not in the index.
    /// - `Ok(Some(blob))` with the (decompressed) bytes on hit.
    /// - `Err(_)` on decompression failure or corrupt extents.
    ///
    /// The returned [`PakBlob`] is `Cow::Borrowed` on the
    /// uncompressed path (zero-copy from mmap) and `Cow::Owned` on
    /// the zstd path (the decompression target).
    pub fn lookup(&self, id: &AssetId) -> Result<Option<PakBlob<'_>>, PakError> {
        let Some(entry) = self.index.lookup(id) else {
            return Ok(None);
        };
        Ok(Some(self.read_blob(entry)?))
    }

    /// Iterate `(IndexEntry, blob)` pairs for every asset, in
    /// sorted-id order. Used by asset-store for cook audit dumps.
    pub fn iter(&self) -> impl Iterator<Item = (&IndexEntry, Result<PakBlob<'_>, PakError>)> {
        self.index
            .entries()
            .iter()
            .map(move |entry| (entry, self.read_blob(entry)))
    }

    fn read_blob(&self, entry: &IndexEntry) -> Result<PakBlob<'_>, PakError> {
        let bytes = self.backing.as_slice();
        let start = entry.offset as usize;
        let end = start + entry.compressed_length as usize;
        // The from_backing pre-validates extents, so this slice is
        // safe; the assert is to surface a corruption-after-open
        // bug if any reader path bypassed validation.
        debug_assert!(end <= bytes.len());
        let slice = &bytes[start..end];

        match self.header.compression_algo {
            CompressionAlgo::None => Ok(Cow::Borrowed(slice)),
            CompressionAlgo::Zstd => {
                // Pre-size the decompression target using the
                // index's `uncompressed_length` hint. This avoids
                // the streaming decompressor doubling its buffer.
                let mut out = Vec::with_capacity(entry.uncompressed_length as usize);
                // `zstd::bulk::decompress_to_buffer` requires a
                // pre-extended target; using the streaming API keeps
                // the code simple at a small allocation cost.
                let mut decoder =
                    zstd::stream::read::Decoder::new(slice).map_err(PakError::zstd)?;
                std::io::copy(&mut decoder, &mut out).map_err(PakError::zstd)?;
                Ok(Cow::Owned(out))
            }
        }
    }

    /// Look up a blob by [`AssetKind`]. Returns the first matching
    /// entry by sorted-id order. Convenience for one-of-a-kind
    /// assets (the cooked main scene, the cooked plugin manifest);
    /// if a kind has multiple entries the caller must use
    /// [`Self::iter`] + filter.
    #[must_use]
    pub fn first_of_kind(
        &self,
        kind: AssetKind,
    ) -> Option<(&IndexEntry, Result<PakBlob<'_>, PakError>)> {
        self.index
            .entries()
            .iter()
            .find(|e| e.kind == kind)
            .map(move |entry| (entry, self.read_blob(entry)))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::writer::PakWriter;

    #[test]
    fn open_truncated_file_errors() {
        let err = PakReader::open_bytes(vec![0u8; 16]).unwrap_err();
        match err {
            PakError::Truncated { got, need } => {
                assert_eq!(got, 16);
                assert_eq!(need, HEADER_SIZE);
            }
            other => panic!("expected Truncated, got {other:?}"),
        }
    }

    #[test]
    fn open_bad_magic_errors() {
        let mut bytes = vec![0u8; HEADER_SIZE + 8];
        bytes[0..4].copy_from_slice(b"XXXX");
        let err = PakReader::open_bytes(bytes).unwrap_err();
        assert!(matches!(err, PakError::BadMagic { .. }));
    }

    #[test]
    fn header_and_index_parse_after_writer() {
        let mut w = PakWriter::new();
        w.add_asset_auto_id(AssetKind::Mesh, b"alpha".to_vec());
        w.add_asset_auto_id(AssetKind::Mesh, b"bravo".to_vec());
        let bytes = w.finish().unwrap();
        let r = PakReader::open_bytes(bytes).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r.header().compression_algo, CompressionAlgo::Zstd);
    }

    #[test]
    fn iter_yields_entries_in_sorted_order() {
        let mut w = PakWriter::new();
        for i in 0u8..10 {
            w.add_asset_auto_id(AssetKind::Opaque, vec![i; 32]);
        }
        let bytes = w.finish().unwrap();
        let r = PakReader::open_bytes(bytes).unwrap();
        let entries: Vec<_> = r.iter().map(|(e, _)| *e).collect();
        for w in entries.windows(2) {
            assert!(w[0].asset_id < w[1].asset_id);
        }
    }

    #[test]
    fn lookup_miss_returns_none() {
        let mut w = PakWriter::new();
        w.add_asset_auto_id(AssetKind::Mesh, b"only".to_vec());
        let bytes = w.finish().unwrap();
        let r = PakReader::open_bytes(bytes).unwrap();
        let id = AssetId::from_bytes(b"different");
        assert!(r.lookup(&id).unwrap().is_none());
    }
}
