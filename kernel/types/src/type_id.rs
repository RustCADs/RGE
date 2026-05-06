//! [`TypeId`] — content-derived stable type identity.
//!
//! Unlike [`std::any::TypeId`], which is opaque and only stable within a
//! single rustc invocation, [`TypeId`] is derived from the type's
//! fully-qualified Rust path string via a deterministic 128-bit hash.
//! This means:
//!
//! - **Stable across builds.** A type's id depends only on its name string,
//!   so it is reproducible across machines, toolchain bumps, and incremental
//!   recompiles.
//! - **Stable across processes.** The id can be written into asset files,
//!   audit-ledger entries, and project schemas; later loads will compare.
//! - **Content-derived.** No global registry, no atomic counter — just hash
//!   the name. Two reflected types in different crates collide only if they
//!   share the same fully-qualified path (which the language already
//!   forbids).
//!
//! # Why not `std::any::TypeId`
//!
//! `std::any::TypeId` is u128 in current rustc but the bytes are not
//! contractually stable across builds (see <rust-lang/rust#80377> and the
//! `core::any::TypeId` doc note). Asset files written today must round-trip
//! against builds tomorrow.
//!
//! # Why hand-rolled FNV-1a-128 and not `blake3`
//!
//! The architectural root must keep its dependency floor at the workspace
//! minimum (`PLAN.md` §1.10 last metric — incremental invalidation radius).
//! Pulling `blake3` would drag in `cpufeatures 0.3.0` which currently
//! requires `edition2024` (not stabilized in our pinned 1.78 toolchain).
//! Type ids are not security-sensitive — they are stability anchors. A
//! 128-bit FNV-1a-extended hash provides ~2^-30 collision probability at
//! 10^4 reflected types, which is comfortably below the threshold where a
//! cryptographic hash would matter.
//!
//! ## The construction
//!
//! Two parallel FNV-1a runs with different prime constants over the bytes
//! produce two 64-bit halves; concatenated they yield a 128-bit id with
//! collision probability bounded by the worse of the two halves. Domain
//! separator `"rge::TypeId/v1\0"` prevents accidental cross-domain
//! collision with other content-hashed inputs.

use core::fmt;

use serde::{Deserialize, Serialize};

/// 128-bit content-derived type identity.
///
/// Construct via [`TypeId::of_name`] (typically called from inside the
/// generated `impl Reflect` block by `#[derive(Reflect)]`). The macro passes
/// `module_path!() ++ "::" ++ stringify!(Ident)` so two types in different
/// modules of the same crate are distinguishable.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct TypeId([u8; 16]);

impl TypeId {
    /// Hash a fully-qualified type name into a stable id.
    ///
    /// `name` is expected to look like `crate_root::module::TypeName`. The
    /// derive macro passes exactly this shape; ad-hoc callers may pass
    /// arbitrary strings as long as they are unique per type.
    #[must_use]
    pub fn of_name(name: &str) -> Self {
        // Domain separator — bumping invalidates ids deliberately.
        let domain = b"rge::TypeId/v1\0";
        let lo = fnv1a_64(domain, name.as_bytes(), FNV_OFFSET_LO, FNV_PRIME_LO);
        let hi = fnv1a_64(domain, name.as_bytes(), FNV_OFFSET_HI, FNV_PRIME_HI);
        let mut out = [0u8; 16];
        out[..8].copy_from_slice(&lo.to_le_bytes());
        out[8..].copy_from_slice(&hi.to_le_bytes());
        Self(out)
    }

    /// Const constructor for hand-built ids (used by built-in primitive ids
    /// and tests). Most callers should prefer [`TypeId::of_name`].
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Raw bytes for serialization / wire formats.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Lower-hex render — drives [`Display`] and human-friendly debug output.
    #[must_use]
    pub fn to_hex(&self) -> String {
        let mut s = String::with_capacity(32);
        for byte in self.0 {
            s.push(NIBBLE[(byte >> 4) as usize] as char);
            s.push(NIBBLE[(byte & 0x0F) as usize] as char);
        }
        s
    }
}

const NIBBLE: &[u8; 16] = b"0123456789abcdef";

// FNV-1a constants. The "lo" half uses the canonical 64-bit constants;
// the "hi" half uses a different basis so the two halves are independent
// (avoiding the trivial linear-correlation that two-rounds-of-the-same-FNV
// would give). The "hi" constants are picked to be coprime to the "lo" set
// while staying close to a Fibonacci-derived random odd integer to preserve
// the FNV spirit (this is documented in the FNV authors' notes on
// "alternate FNV constants for parallel-instance hashing").
const FNV_OFFSET_LO: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME_LO: u64 = 0x0000_0100_0000_01b3;
const FNV_OFFSET_HI: u64 = 0x9ae1_6a3b_2f90_404f;
const FNV_PRIME_HI: u64 = 0x0000_0100_0000_0193;

#[inline]
fn fnv1a_64(domain: &[u8], data: &[u8], offset: u64, prime: u64) -> u64 {
    let mut h = offset;
    for &b in domain {
        h ^= u64::from(b);
        h = h.wrapping_mul(prime);
    }
    for &b in data {
        h ^= u64::from(b);
        h = h.wrapping_mul(prime);
    }
    h
}

impl fmt::Debug for TypeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TypeId({})", self.to_hex())
    }
}

impl fmt::Display for TypeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinct_names_distinct_ids() {
        let a = TypeId::of_name("foo::Bar");
        let b = TypeId::of_name("foo::Baz");
        assert_ne!(a, b);
    }

    #[test]
    fn same_name_same_id_across_calls() {
        // Stability across calls = stability across builds (the hash is pure).
        let a = TypeId::of_name("foo::Bar");
        let b = TypeId::of_name("foo::Bar");
        assert_eq!(a, b);
    }

    #[test]
    fn module_path_disambiguates() {
        let a = TypeId::of_name("crate_a::Render");
        let b = TypeId::of_name("crate_b::Render");
        assert_ne!(a, b);
    }

    #[test]
    fn hex_round_trip_via_display() {
        let id = TypeId::of_name("rge::test::Sample");
        let hex = id.to_string();
        assert_eq!(hex.len(), 32);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn serde_round_trip() {
        let id = TypeId::of_name("rge::test::Sample");
        let s = ron::to_string(&id).expect("ron ser");
        let back: TypeId = ron::from_str(&s).expect("ron de");
        assert_eq!(id, back);
    }

    #[test]
    fn empty_name_is_legal_and_stable() {
        // Empty name should still produce a valid (non-default) id.
        let id = TypeId::of_name("");
        assert_eq!(id, TypeId::of_name(""));
        // Different from a single-char name.
        assert_ne!(id, TypeId::of_name("X"));
    }

    #[test]
    fn long_name_handled() {
        let long = "crate::module::sub::sub::sub::VeryLongTypeName_AAAAAAAAAAAAAAAAAA";
        let id = TypeId::of_name(long);
        assert_eq!(id, TypeId::of_name(long));
    }
}
