//! Perf smoke for the §13.4 / W15 exit criterion:
//!
//! "100MB pak loads in <500ms (mmap + decompress on demand)."
//!
//! "Loads" here means *open the pak and parse the index* — the
//! decompression-on-demand contract means we DON'T touch blob
//! bytes until lookup. So this test:
//!
//! 1. Cooks a ~100 MB pak to a tempfile (offline; outside the
//!    wall-clock budget).
//! 2. Times `PakReader::open` only.
//! 3. Asserts that single open is well under 500 ms.
//!
//! Run with `cargo test -p rge-pak-format --release -- --ignored perf_load_100mb_under_500ms --nocapture`.
//! Marked `#[ignore]` so CI default doesn't pay the ~10s cook cost.

use std::io::Write as _;
use std::time::Instant;

use rge_pak_format::{AssetKind, PakReader, PakWriter};
use tempfile::NamedTempFile;

#[test]
#[ignore = "perf gate; ~10s cook cost — opt in via `cargo test -- --ignored`"]
fn perf_load_100mb_under_500ms() {
    // Build a pak whose decompressed payload sums to ~100 MB. We
    // use highly-compressible filler (constant byte) so the
    // resulting on-disk size stays bounded; the load gate is about
    // index-parse cost vs file size, NOT compression cost.
    //
    // 1000 blobs × 100 KB each = 100 MB uncompressed.
    const BLOB_COUNT: u32 = 1000;
    const BLOB_SIZE: usize = 100 * 1024;

    // Use uncompressed codec so the on-disk size matches the
    // logical 100 MB. With zstd + repeating-byte filler the
    // result would compress to ~80 KB and not exercise the
    // mmap-of-large-file path.
    let mut w = PakWriter::with_compression(rge_pak_format::CompressionAlgo::None);
    for i in 0..BLOB_COUNT {
        let mut blob = Vec::with_capacity(BLOB_SIZE);
        // Pseudo-random fill via xorshift so content hashes are
        // unique AND zstd-incompressible if we ever flip back.
        let mut state: u32 = i.wrapping_add(1);
        blob.resize(BLOB_SIZE, 0);
        for slot in blob.chunks_exact_mut(4) {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            slot.copy_from_slice(&state.to_le_bytes());
        }
        w.add_asset_auto_id(AssetKind::Opaque, blob);
    }
    let bytes = w.finish().unwrap();
    eprintln!(
        "cooked pak size: {} bytes ({} MB)",
        bytes.len(),
        bytes.len() / (1024 * 1024)
    );

    let mut tmp = NamedTempFile::new().unwrap();
    tmp.write_all(&bytes).unwrap();
    tmp.flush().unwrap();

    // The actual gate: time the open.
    let t0 = Instant::now();
    let r = PakReader::open(tmp.path()).unwrap();
    let open_elapsed = t0.elapsed();
    eprintln!(
        "open elapsed: {:.3} ms ({} entries, {} backing bytes)",
        open_elapsed.as_secs_f64() * 1000.0,
        r.len(),
        bytes.len()
    );

    assert_eq!(r.len(), BLOB_COUNT as usize);
    assert!(
        open_elapsed.as_millis() < 500,
        "load gate failed: open took {} ms, budget is 500 ms",
        open_elapsed.as_millis()
    );

    // Spot-check: a lookup decompresses just one blob.
    let entries: Vec<_> = r.index().entries().to_vec();
    let probe = &entries[BLOB_COUNT as usize / 2];
    let t1 = Instant::now();
    let blob = r.lookup(&probe.asset_id).unwrap().unwrap();
    let lookup_elapsed = t1.elapsed();
    eprintln!(
        "single-blob lookup elapsed: {:.3} ms (decompressed {} bytes)",
        lookup_elapsed.as_secs_f64() * 1000.0,
        blob.len()
    );
    assert_eq!(blob.len(), BLOB_SIZE);
}
