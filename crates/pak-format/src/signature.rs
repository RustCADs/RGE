//! Ed25519 signature stub for marketplace integrity.
//!
//! # Phase
//!
//! Phase-5 (marketplace) makes this real. v0.0.1 keeps a fixed-size
//! 64-byte signature region at the tail of every pak; for unsigned
//! paks the region is zero-filled, which preserves the determinism
//! gate (two cooks of identical sources → byte-identical pak)
//! because zeros don't depend on any unspecified state.
//!
//! Why ship the stub now: the wire layout MUST reserve the bytes
//! at v1 of the format, otherwise turning on signing in Phase-5
//! would break every pre-Phase-5 pak. See PLAN.md §1.6.4.
//!
//! # Sign-over surface
//!
//! When real signing arrives, the signature covers
//! `header_bytes ++ index_region_bytes` — i.e. the first
//! `HEADER_SIZE + 8 + N * INDEX_ENTRY_SIZE` bytes of the file.
//! That commits the writer to a specific blob layout (every entry
//! references blob bytes via offset+length+content-hash, so
//! covering the index transitively covers all blobs). It also
//! avoids re-hashing 100 MB of blob bytes on verification.
//!
//! [`verify_signature`] is a no-op for now; [`SIGNATURE_SIZE`] is
//! the wire constant.

use ed25519_dalek::Signature;

use crate::PakError;

/// Trailing signature region size, bytes. Matches Ed25519 signature
/// length. **Wire constant.** Never change without bumping
/// `header::ENGINE_VERSION`.
pub const SIGNATURE_SIZE: usize = Signature::BYTE_SIZE;

/// Compile-time assertion that `Signature::BYTE_SIZE == 64`. If
/// `ed25519-dalek` ever changes the constant this trips and the
/// tail-region size needs explicit re-evaluation.
const _: [(); 64] = [(); SIGNATURE_SIZE];

/// Verify the signature region of an opened pak.
///
/// At v0.0.1 this is a stub: returns `Ok(())` when the region is
/// all-zeros (unsigned pak), and `Ok(())` regardless for a non-zero
/// region (treated as "signed but Phase-5 verification not yet
/// implemented"). Phase-5 will:
///
/// 1. Take a `&VerifyingKey` parameter.
/// 2. Compute `blake3(header_bytes ++ index_region_bytes)`.
/// 3. Call `verifying_key.verify(&hash, signature)`.
/// 4. Return `Err(PakError::SignatureMismatch)` on failure.
///
/// # Errors
///
/// Currently never errors. The signature on the return type lets
/// the Phase-5 implementation slot in without breaking callers.
#[allow(clippy::unnecessary_wraps)] // Phase-5 will return Err — forward-compat for callers
pub fn verify_signature(_signature_bytes: &[u8; SIGNATURE_SIZE]) -> Result<(), PakError> {
    // Stub. See module docstring.
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn signature_size_is_sixty_four() {
        // Wire-constant pin.
        assert_eq!(SIGNATURE_SIZE, 64);
    }

    #[test]
    fn verify_zero_signature_is_ok() {
        verify_signature(&[0u8; SIGNATURE_SIZE]).unwrap();
    }

    #[test]
    fn verify_nonzero_signature_is_ok_at_v0() {
        // At v0.0.1 we don't reject non-zero signatures (they're
        // the "signed by Phase-5 but verifier-not-yet-built" case).
        let mut sig = [0u8; SIGNATURE_SIZE];
        sig[0] = 0x01;
        verify_signature(&sig).unwrap();
    }
}
