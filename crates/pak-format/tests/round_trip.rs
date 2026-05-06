//! Round-trip & lookup tests — covers the W15 §exit-criteria items:
//!
//! 1. "Write 100 dummy assets to `.rge-pak`; read back byte-identical."
//! 2. "Index lookup O(log n) on 10k entries." Asserted by sortedness +
//!    binary-search correctness on a 10k-entry index; we don't
//!    instrument the actual log-n curve (no benchmark crate yet),
//!    but the search runs through `Vec::binary_search_by` so the
//!    complexity is structural.
//!
//! Both use [`PakReader::open_bytes`] to keep the test hermetic
//! (no tempdir, no fs).

use rge_pak_format::{AssetKind, PakReader, PakWriter};

#[test]
fn hundred_dummy_assets_round_trip_byte_identical() {
    let payloads: Vec<Vec<u8>> = (0u32..100)
        .map(|i| {
            // Vary content per index so each id is unique.
            let mut blob = Vec::with_capacity(256);
            blob.extend_from_slice(&i.to_le_bytes());
            // Fill the rest with the index byte to give zstd
            // something to compress (otherwise 100 tiny random
            // blobs all hit the codec's no-op floor).
            blob.resize(256, (i & 0xFF) as u8);
            blob
        })
        .collect();

    let mut w = PakWriter::new();
    let mut ids = Vec::with_capacity(payloads.len());
    for blob in &payloads {
        ids.push(w.add_asset_auto_id(AssetKind::Opaque, blob.clone()));
    }
    let bytes = w.finish().unwrap();

    let r = PakReader::open_bytes(bytes).unwrap();
    assert_eq!(r.len(), 100);

    // Every id round-trips to the EXACT input bytes.
    for (id, original) in ids.iter().zip(payloads.iter()) {
        let recovered = r.lookup(id).unwrap().expect("present in pak");
        assert_eq!(
            recovered.as_ref(),
            original.as_slice(),
            "blob for {id} round-tripped non-identically"
        );
    }
}

#[test]
fn ten_thousand_entries_lookup_correctness() {
    // O(log n) is structural via `binary_search_by`; we can at
    // least confirm correctness on every entry across a 10k pak.
    // We use uncompressed mode to keep the test fast — the codec
    // path is exercised in `hundred_dummy_assets_round_trip_byte_identical`.
    use rge_pak_format::CompressionAlgo;

    let mut w = PakWriter::with_compression(CompressionAlgo::None);
    let mut ids = Vec::with_capacity(10_000);
    for i in 0u32..10_000 {
        // Each blob is 8 bytes — content-hash is unique because
        // we splat the iter index into both u32 halves.
        let mut blob = Vec::with_capacity(8);
        blob.extend_from_slice(&i.to_le_bytes());
        blob.extend_from_slice(&(!i).to_le_bytes());
        ids.push(w.add_asset_auto_id(AssetKind::Opaque, blob));
    }
    let bytes = w.finish().unwrap();
    let r = PakReader::open_bytes(bytes).unwrap();
    assert_eq!(r.len(), 10_000);

    // Spot-check: every id in the input set resolves to a real blob.
    // Walk every id (10k * O(log n) ≈ 10k * 14 ops = 140k ops,
    // sub-millisecond on any 2010s CPU).
    for id in &ids {
        assert!(r.lookup(id).unwrap().is_some());
    }

    // Negative spot-check: a synthesised id that wasn't staged
    // resolves to `None`.
    let phantom = rge_pak_format::AssetId::from_bytes(b"never-staged");
    assert!(r.lookup(&phantom).unwrap().is_none());
}

#[test]
fn index_is_sorted_after_random_input() {
    // The writer sorts internally; verify the on-disk index is
    // strictly ascending regardless of input order.
    let mut w = PakWriter::new();
    // Pseudorandom input order without bringing in `rand`.
    let order: [u32; 32] = [
        17, 3, 28, 5, 11, 22, 31, 8, 14, 20, 1, 6, 10, 19, 25, 30, 0, 4, 9, 13, 16, 21, 27, 7, 2,
        12, 15, 18, 23, 24, 26, 29,
    ];
    for n in order {
        w.add_asset_auto_id(AssetKind::Opaque, n.to_le_bytes().to_vec());
    }
    let bytes = w.finish().unwrap();
    let r = PakReader::open_bytes(bytes).unwrap();
    for w in r.index().entries().windows(2) {
        assert!(
            w[0].asset_id < w[1].asset_id,
            "index must be strictly ascending"
        );
    }
}
