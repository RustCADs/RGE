//! mmap'd reader tests + perf smoke for the §13.4 100MB-load gate.
//!
//! "100MB pak loads in <500ms (mmap + decompress on demand)."
//!
//! The exit-criteria perf number is meaningful only on a target
//! machine, so this test runs a smaller load (a few MB) and asserts
//! the *open* cost is fast (mmap is constant-time vs file size on
//! Linux/macOS/Win). The full 100 MB benchmark lives outside
//! `cargo test` (it takes too long for default-CI).

use std::time::Instant;

use rge_pak_format::{AssetKind, PakReader, PakWriter};
use tempfile::NamedTempFile;

fn write_temp_pak(payloads: &[Vec<u8>]) -> NamedTempFile {
    let mut w = PakWriter::new();
    for blob in payloads {
        w.add_asset_auto_id(AssetKind::Opaque, blob.clone());
    }
    let bytes = w.finish().unwrap();
    let mut tmp = NamedTempFile::new().unwrap();
    use std::io::Write;
    tmp.write_all(&bytes).unwrap();
    tmp.flush().unwrap();
    tmp
}

#[test]
fn mmap_open_round_trips() {
    let payloads: Vec<Vec<u8>> = (0..20).map(|i| vec![i as u8; 1024]).collect();
    let tmp = write_temp_pak(&payloads);

    let r = PakReader::open(tmp.path()).unwrap();
    assert_eq!(r.len(), payloads.len());

    // Resolve every entry. Walks the index in sorted-id order,
    // so we don't know the input→position mapping; check that
    // every recovered blob is one of the inputs.
    let mut recovered: Vec<Vec<u8>> = Vec::new();
    for (entry, blob) in r.iter() {
        let blob = blob.unwrap();
        recovered.push(blob.into_owned());
        // entry.uncompressed_length is consistent with what the
        // writer staged.
        assert!(entry.uncompressed_length > 0);
    }
    recovered.sort();
    let mut expected = payloads.clone();
    expected.sort();
    assert_eq!(recovered, expected);
}

#[test]
fn mmap_open_is_fast_for_multi_mb_pak() {
    // Smoke the perf gate. We don't assert <500ms for 100MB here
    // (CI machines vary wildly), but we DO assert the open of a
    // ~4MB pak is sub-100ms on any reasonable machine. mmap is
    // O(1) in file size on all three platforms; the only work is
    // the index parse.
    //
    // 4 MB total at 4 KB per blob = 1024 blobs; index region is
    // 8 + 1024*60 = 61448 bytes. Well within the §13.4 budget.
    let payloads: Vec<Vec<u8>> = (0u32..1024)
        .map(|i| {
            let mut blob = Vec::with_capacity(4 * 1024);
            blob.extend_from_slice(&i.to_le_bytes());
            blob.resize(4 * 1024, (i & 0xFF) as u8);
            blob
        })
        .collect();
    let tmp = write_temp_pak(&payloads);

    let t0 = Instant::now();
    let r = PakReader::open(tmp.path()).unwrap();
    let open_us = t0.elapsed().as_micros();
    assert_eq!(r.len(), 1024);

    // 100ms = 100_000us. Generous bound; mmap+index typically
    // <5ms locally. Failure here means something is reading the
    // whole file at open time (defeats lazy decompression).
    assert!(
        open_us < 100_000,
        "mmap open of 4MB pak took {open_us}us — expected <100ms"
    );
}

#[test]
fn lazy_decompression_per_lookup() {
    // Exercise the lazy-decompress contract: open is fast,
    // per-lookup cost is per-blob (not per-pak). We can't measure
    // memory residency from a unit test, but we can confirm a
    // lookup of a single blob doesn't error and produces the
    // expected bytes.
    let payloads: Vec<Vec<u8>> = (0..16).map(|i| vec![i as u8; 16 * 1024]).collect();
    let tmp = write_temp_pak(&payloads);
    let r = PakReader::open(tmp.path()).unwrap();

    // Look up every entry one at a time.
    let entries: Vec<_> = r.index().entries().to_vec();
    for entry in &entries {
        let blob = r.lookup(&entry.asset_id).unwrap().unwrap();
        // Recover the per-payload byte from the first byte (we
        // filled with `i` and the first byte of the input is `i`).
        let first_byte = blob[0];
        // It should be one of 0..16.
        assert!(first_byte < 16);
        // And it should occupy the full 16 KB.
        assert_eq!(blob.len(), 16 * 1024);
    }
}

#[test]
fn open_via_path_and_via_bytes_yield_same_index() {
    let payloads: Vec<Vec<u8>> = (0..8).map(|i| vec![i as u8; 64]).collect();
    let mut w = PakWriter::new();
    for p in &payloads {
        w.add_asset_auto_id(AssetKind::Opaque, p.clone());
    }
    let bytes = w.finish().unwrap();

    // Round trip through the filesystem.
    let mut tmp = NamedTempFile::new().unwrap();
    use std::io::Write;
    tmp.write_all(&bytes).unwrap();
    tmp.flush().unwrap();

    let r_mmap = PakReader::open(tmp.path()).unwrap();
    let r_bytes = PakReader::open_bytes(bytes).unwrap();
    assert_eq!(r_mmap.len(), r_bytes.len());
    for (a, b) in r_mmap
        .index()
        .entries()
        .iter()
        .zip(r_bytes.index().entries())
    {
        assert_eq!(a, b);
    }
}
