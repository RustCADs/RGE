//! Fixed 32-byte top-of-file header for `.rge-pak`.
//!
//! Spec: PLAN.md §1.6.4 + W15 dispatch package §3.
//!
//! ```text
//! offset  size  field                              notes
//! ------  ----  ---------------------------------  ----------------------------
//! 0x00    4     magic                              b"RGEP" (literal ASCII)
//! 0x04    4     engine_version (u32 LE)            bumps with breaking-format change
//! 0x08    4     player_state_schema_version (u32)  bumps independently per §1.6.9
//! 0x0C    4     flags (u32 LE)                     bitfield (currently zero)
//! 0x10    1     compression_algo (u8 enum)         0=None, 1=Zstd
//! 0x11    15    reserved                           must be zero
//! ------  ----  ----------------------------------
//! total = 32 bytes
//! ```
//!
//! # Why hand-rolled
//!
//! `.rge-pak` is a fixed-layout binary container. Generic codec
//! crates (`bincode`, `serde_binary`) would silently introduce
//! length prefixes / type tags that break wire compatibility when
//! the cooker on disk-A and the loader on disk-B disagree on serde
//! version. The CTB writer in rustforge took the same position and
//! the same justification applies here verbatim:
//!
//! `// adapted from rustforge::crates::manufacturing-format-ctb on
//! 2026-05-05 — header+index pattern for .rge-pak`

use std::io::{Read, Write};

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

use crate::PakError;

/// Magic bytes at file start. Plain ASCII `b"RGEP"`.
///
/// Stored as a literal byte sequence (NOT a `u32`) so a hex-dump of
/// the file shows the magic in cleartext at offset 0 — a small but
/// real diagnostic affordance for "is this even a pak?" forensics.
pub const MAGIC: [u8; 4] = *b"RGEP";

/// Engine format version. Bumps when the wire layout changes in a
/// way that breaks readers. v1 == this initial implementation.
pub const ENGINE_VERSION: u32 = 1;

/// Player-state schema version (independent of [`ENGINE_VERSION`] —
/// PLAN.md §1.6.9). v1 placeholder; W14 (rge-data schema) owns the
/// bump policy.
pub const PLAYER_STATE_SCHEMA_VERSION: u32 = 1;

/// Header is exactly 32 bytes. This is a wire constant, NOT
/// `size_of::<PakHeader>()`. The struct uses native rust layout for
/// in-memory ergonomics; serialisation is handled by [`write_into`]
/// / [`read_from`].
pub const HEADER_SIZE: usize = 32;

/// Reserved-bytes count after the compression-algo byte.
const RESERVED_LEN: usize = 15;

/// Compression algorithm enum. The byte at offset 0x10 in the
/// header. Unrecognised values are a hard error (see [`PakError::UnknownCompressionAlgo`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CompressionAlgo {
    /// No compression. Blobs stored as-is. Used by tests and tiny
    /// paks where zstd overhead exceeds savings.
    None = 0,
    /// `zstd` (Facebook). Default for production. Single-threaded
    /// at a fixed level for determinism — see [`writer::ZSTD_LEVEL`].
    Zstd = 1,
}

impl CompressionAlgo {
    /// Convert from on-wire byte. Anything other than 0/1 is an error.
    pub fn from_byte(b: u8) -> Result<Self, PakError> {
        match b {
            0 => Ok(Self::None),
            1 => Ok(Self::Zstd),
            other => Err(PakError::UnknownCompressionAlgo(other)),
        }
    }

    /// Inverse of [`from_byte`].
    #[must_use]
    pub fn to_byte(self) -> u8 {
        self as u8
    }
}

/// In-memory representation of the fixed 32-byte header. Each field
/// maps 1:1 to the wire layout in this module's docstring.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PakHeader {
    /// `ENGINE_VERSION` baked at write time.
    pub engine_version: u32,
    /// `PLAYER_STATE_SCHEMA_VERSION` baked at write time.
    pub player_state_schema_version: u32,
    /// Reserved bitfield; currently zero. Future: e.g. "has trailing signature".
    pub flags: u32,
    /// Codec used for blob payloads.
    pub compression_algo: CompressionAlgo,
}

impl PakHeader {
    /// Build a header at the current engine + schema versions, no
    /// flags set, defaulting to zstd. Convenience for the writer's
    /// happy path.
    #[must_use]
    pub fn current(compression_algo: CompressionAlgo) -> Self {
        Self {
            engine_version: ENGINE_VERSION,
            player_state_schema_version: PLAYER_STATE_SCHEMA_VERSION,
            flags: 0,
            compression_algo,
        }
    }

    /// Serialise the 32-byte header into `w`. Reserved bytes are
    /// emitted as zeros; this is load-bearing for determinism.
    pub fn write_into<W: Write>(&self, w: &mut W) -> Result<(), PakError> {
        w.write_all(&MAGIC)?;
        w.write_u32::<LittleEndian>(self.engine_version)?;
        w.write_u32::<LittleEndian>(self.player_state_schema_version)?;
        w.write_u32::<LittleEndian>(self.flags)?;
        w.write_u8(self.compression_algo.to_byte())?;
        // Reserved 15 bytes — must be zero on every write so two cooks
        // of identical inputs are byte-identical.
        w.write_all(&[0u8; RESERVED_LEN])?;
        Ok(())
    }

    /// Deserialise a 32-byte header from `r`. Validates the magic
    /// and the compression-algo enum; does NOT validate the version
    /// fields (caller decides whether to migrate or reject).
    pub fn read_from<R: Read>(r: &mut R) -> Result<Self, PakError> {
        let mut magic = [0u8; 4];
        r.read_exact(&mut magic)?;
        if magic != MAGIC {
            return Err(PakError::BadMagic { got: magic });
        }
        let engine_version = r.read_u32::<LittleEndian>()?;
        let player_state_schema_version = r.read_u32::<LittleEndian>()?;
        let flags = r.read_u32::<LittleEndian>()?;
        let algo_byte = r.read_u8()?;
        let compression_algo = CompressionAlgo::from_byte(algo_byte)?;
        // Drain reserved bytes; we don't validate they're zero, just
        // that they exist (forward-compat: a future header bump may
        // claim them).
        let mut reserved = [0u8; RESERVED_LEN];
        r.read_exact(&mut reserved)?;
        Ok(Self {
            engine_version,
            player_state_schema_version,
            flags,
            compression_algo,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn header_size_constant_is_thirty_two() {
        // Wire-size invariant. If you bump this you've changed the
        // file format; expect a fight with the Phase-5 marketplace
        // signing logic.
        assert_eq!(HEADER_SIZE, 32);
    }

    #[test]
    fn header_round_trips_byte_identical() {
        let h = PakHeader::current(CompressionAlgo::Zstd);
        let mut buf = Vec::new();
        h.write_into(&mut buf).unwrap();
        assert_eq!(buf.len(), HEADER_SIZE);
        let mut cursor = std::io::Cursor::new(&buf);
        let parsed = PakHeader::read_from(&mut cursor).unwrap();
        assert_eq!(parsed, h);
    }

    #[test]
    fn magic_appears_at_offset_zero_in_ascii() {
        let h = PakHeader::current(CompressionAlgo::None);
        let mut buf = Vec::new();
        h.write_into(&mut buf).unwrap();
        assert_eq!(&buf[0..4], b"RGEP");
    }

    #[test]
    fn reserved_bytes_are_zero_after_write() {
        let h = PakHeader::current(CompressionAlgo::Zstd);
        let mut buf = Vec::new();
        h.write_into(&mut buf).unwrap();
        // Reserved zone is offsets 0x11..0x20.
        assert!(buf[0x11..0x20].iter().all(|&b| b == 0));
    }

    #[test]
    fn bad_magic_is_detected() {
        let mut buf = vec![b'X'; HEADER_SIZE];
        let err = PakHeader::read_from(&mut std::io::Cursor::new(&buf)).unwrap_err();
        match err {
            PakError::BadMagic { got } => assert_eq!(got, [b'X', b'X', b'X', b'X']),
            other => panic!("expected BadMagic, got {other:?}"),
        }
        // Sanity: also fix it and parse cleanly.
        buf[0..4].copy_from_slice(b"RGEP");
        // engine_version=1 (LE)
        buf[4] = 1;
        // compression_algo byte=0 (None)
        buf[0x10] = 0;
        let _ = PakHeader::read_from(&mut std::io::Cursor::new(&buf)).unwrap();
    }

    #[test]
    fn unknown_compression_algo_is_detected() {
        let mut buf = vec![0u8; HEADER_SIZE];
        buf[0..4].copy_from_slice(b"RGEP");
        // engine_version=1
        buf[4] = 1;
        buf[0x10] = 99; // not a known algo
        let err = PakHeader::read_from(&mut std::io::Cursor::new(&buf)).unwrap_err();
        match err {
            PakError::UnknownCompressionAlgo(b) => assert_eq!(b, 99),
            other => panic!("expected UnknownCompressionAlgo, got {other:?}"),
        }
    }

    #[test]
    fn two_writes_of_same_header_are_byte_identical() {
        // Determinism gate at the smallest unit. The full-pak gate
        // lives in `tests/determinism_gate.rs`.
        let h = PakHeader::current(CompressionAlgo::Zstd);
        let mut a = Vec::new();
        let mut b = Vec::new();
        h.write_into(&mut a).unwrap();
        h.write_into(&mut b).unwrap();
        assert_eq!(a, b);
    }
}
