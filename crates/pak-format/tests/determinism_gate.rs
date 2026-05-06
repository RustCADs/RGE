//! Determinism gate (PLAN.md §13.4 + W15 exit criterion).
//!
//! "Two writes of identical assets in identical order → byte-
//! identical pak". This is the CI-blocking property: cook
//! reproducibility is the foundation that asset-store, marketplace
//! signing, and content-addressed delta updates all build on.
//!
//! If this test ever fails, do NOT relax it — find the source of
//! non-determinism (timestamps, PIDs, random IVs, multithreaded
//! interleaving, hashmap iteration order) and fix the root cause.

use rge_pak_format::{AssetId, AssetKind, CompressionAlgo, PakWriter};

fn cook_fixture() -> Vec<u8> {
    let mut w = PakWriter::new();
    // A handful of varied blobs to exercise the sort+compress path.
    w.add_asset_auto_id(AssetKind::Mesh, b"mesh-data-aaa".to_vec());
    w.add_asset_auto_id(AssetKind::Texture, vec![0xAB; 1024]);
    w.add_asset_auto_id(AssetKind::Audio, b"audio-bytes".to_vec());
    w.add_asset_auto_id(AssetKind::Material, b"{\"pbr\":true}".to_vec());
    w.add_asset_auto_id(AssetKind::AnimClip, vec![0x7F; 256]);
    w.add_asset_auto_id(AssetKind::Shader, b"@fragment fn fs() {}".to_vec());
    w.add_asset_auto_id(AssetKind::Script, vec![0x00, 0x61, 0x73, 0x6D]); // wasm magic
    w.add_asset_auto_id(AssetKind::Scene, b"scene-blob".to_vec());
    w.add_asset_auto_id(AssetKind::Prefab, b"prefab-blob".to_vec());
    w.finish().unwrap()
}

#[test]
fn two_cooks_of_same_source_are_byte_identical() {
    let a = cook_fixture();
    let b = cook_fixture();
    assert_eq!(
        a, b,
        "two cooks of identical inputs produced different bytes — \
         determinism gate failed"
    );
}

#[test]
fn determinism_holds_across_input_order_permutation() {
    // The sort step inside the writer is supposed to make the
    // staging order irrelevant. Verify by staging the same set in
    // two different orders and comparing bytes.
    let cook_in_order = || {
        let mut w = PakWriter::new();
        for i in 0..32u8 {
            w.add_asset_auto_id(AssetKind::Opaque, vec![i; 64]);
        }
        w.finish().unwrap()
    };
    let cook_reverse_order = || {
        let mut w = PakWriter::new();
        for i in (0..32u8).rev() {
            w.add_asset_auto_id(AssetKind::Opaque, vec![i; 64]);
        }
        w.finish().unwrap()
    };
    assert_eq!(
        cook_in_order(),
        cook_reverse_order(),
        "writer must be input-order-independent"
    );
}

#[test]
fn determinism_holds_for_uncompressed_codec_too() {
    // Mostly a guard against accidentally introducing
    // non-determinism in the no-compression path (e.g. via
    // uninitialised buffer reuse).
    let cook = || {
        let mut w = PakWriter::with_compression(CompressionAlgo::None);
        for i in 0..16u8 {
            w.add_asset_auto_id(AssetKind::Opaque, vec![i ^ 0x55; 32]);
        }
        w.finish().unwrap()
    };
    assert_eq!(cook(), cook());
}

#[test]
fn signature_region_is_zero_for_unsigned_paks() {
    // The trailing 64 bytes are the signature region. For
    // unsigned paks (everything pre-Phase-5) those bytes must be
    // zero — non-zero bytes in this region from random sources
    // (e.g. uninitialised memory) would silently violate
    // determinism.
    let bytes = cook_fixture();
    let sig_region = &bytes[bytes.len() - rge_pak_format::SIGNATURE_SIZE..];
    assert!(
        sig_region.iter().all(|&b| b == 0),
        "signature region must be zero-filled for unsigned paks"
    );
}

#[test]
fn duplicate_id_collapse_is_deterministic() {
    // Duplicate ids stage twice but resolve to the LAST staged
    // payload. Verify two cooks with duplicates produce identical
    // bytes (i.e. the dedup pass is itself deterministic).
    let cook = || {
        let mut w = PakWriter::new();
        let id = AssetId::from_bytes(b"duplicate-key");
        w.add_asset(id, AssetKind::Opaque, b"first".to_vec());
        w.add_asset(id, AssetKind::Opaque, b"second".to_vec());
        w.add_asset(id, AssetKind::Opaque, b"third".to_vec());
        // Plus some other entries so the table isn't trivial.
        w.add_asset_auto_id(AssetKind::Opaque, b"unrelated".to_vec());
        w.finish().unwrap()
    };
    assert_eq!(cook(), cook());
}
