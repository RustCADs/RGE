//! Error type for `.rge-pak` reader/writer.
//!
//! One enum, one place. Per the plan's diagnostics philosophy
//! (PLAN.md §1.7) errors aggregate context rather than fail-first.

use thiserror::Error;

/// Top-level `rge-pak-format` error.
///
/// Variants distinguish *structural* failures (bad magic, truncated
/// region, index out-of-bounds) from *codec* failures (zstd error,
/// IO error). Reader and writer share this type.
#[derive(Debug, Error)]
pub enum PakError {
    /// File does not start with `b"RGEP"`.
    #[error("bad magic: expected b\"RGEP\", got {got:?}")]
    BadMagic {
        /// First four bytes of the file as observed.
        got: [u8; 4],
    },

    /// Header `compression_algo` byte is outside the known enum values.
    #[error("unknown compression algo byte: {0:#04x}")]
    UnknownCompressionAlgo(u8),

    /// File is shorter than the fixed 32-byte header.
    #[error("file truncated: only {got} bytes (need at least {need})")]
    Truncated {
        /// Actual file length in bytes.
        got: usize,
        /// Minimum required length in bytes.
        need: usize,
    },

    /// The index region size implied by `entry_count` exceeds what the
    /// file can contain. Indicates a corrupt or maliciously crafted pak.
    #[error("index out-of-bounds: {entry_count} entries × {entry_size}B = {needed}B but file has {available}B remaining after header")]
    IndexOutOfBounds {
        /// Declared entry count from the index header.
        entry_count: u64,
        /// Per-entry byte size.
        entry_size: usize,
        /// Total bytes the index region would occupy.
        needed: usize,
        /// Bytes available in the file after the header.
        available: usize,
    },

    /// A blob's `(offset, length)` falls outside the file.
    #[error("blob extent out-of-bounds for asset_id {asset_id}: offset={offset} length={length} file_size={file_size}")]
    BlobOutOfBounds {
        /// Asset id formatted as `blake3:<hex>`.
        asset_id: String,
        /// Declared blob start offset.
        offset: u64,
        /// Declared compressed blob length.
        length: u64,
        /// Total file size.
        file_size: u64,
    },

    /// Index is not sorted strictly ascending (writer bug or tampering).
    #[error("index not sorted at entry {index}")]
    IndexNotSorted {
        /// Position in the index where the violation was observed.
        index: usize,
    },

    /// zstd decompression failure.
    #[error("zstd error: {0}")]
    Zstd(String),

    /// Underlying IO error (file open, mmap, write).
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl PakError {
    /// Wrap any error type producing a string into [`PakError::Zstd`].
    /// Used by the writer/reader to flatten zstd's diverse error
    /// surface into a single variant.
    pub(crate) fn zstd<E: std::fmt::Display>(e: E) -> Self {
        Self::Zstd(e.to_string())
    }
}
