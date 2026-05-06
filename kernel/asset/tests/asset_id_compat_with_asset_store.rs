//! Sanity check: `kernel/asset`'s `AssetId` text form matches the
//! `crates/asset-store` convention byte-for-byte.
//!
//! This ensures the future migration (`pub use rge_kernel_asset::AssetId`) is
//! purely mechanical — no data conversion needed.

use rge_kernel_asset::AssetId;

#[test]
fn asset_id_text_form_matches_asset_store_convention() {
    // The asset-store crate ships its own AssetId today (Status.md
    // "canonical owner reconciliation" debt). Their text form is
    // `blake3:<64-hex-lowercase>`. Verify ours matches byte-for-byte so
    // the future migration is mechanical.
    let id = AssetId::from_bytes(b"reference fixture");
    let s = id.to_string();
    assert!(s.starts_with("blake3:"));
    assert_eq!(s.len(), 7 + 64);
    assert!(s[7..]
        .chars()
        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    let parsed: AssetId = s.parse().expect("round-trip");
    assert_eq!(id, parsed);
}

#[test]
fn known_vector_matches_asset_store_cross_machine_determinism() {
    // The asset-store test pins the same empty-input and "abc" vectors.
    // Reproduce them here to guarantee identical output.
    let cases: &[(&[u8], &str)] = &[
        (
            b"",
            "blake3:af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262",
        ),
        (
            b"abc",
            "blake3:6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85",
        ),
    ];
    for (input, expected) in cases {
        let id = AssetId::from_bytes(input);
        assert_eq!(
            id.to_string(),
            *expected,
            "vector mismatch for input len {}",
            input.len()
        );
    }
}
