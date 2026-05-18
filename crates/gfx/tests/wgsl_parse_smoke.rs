//! Shader validation smoke test for Phase 6.3 (GitHub issue #15).
//!
//! Parse-only coverage: each embedded WGSL shader constant exposed by
//! `rge-gfx` must parse cleanly through Naga's WGSL frontend. This guards the
//! shader text against syntax regressions without constructing any GPU
//! adapter, device, or render pipeline.
//!
//! Scope is intentionally narrow — `naga::front::wgsl::parse_str` performs
//! parsing only; this test does not run Naga IR validation, shader linting, or
//! any render-output checks.

use rge_gfx::lit_mesh_pipeline::LIT_MESH_WGSL;
use rge_gfx::mesh_pipeline::MESH_WGSL;
use rge_gfx::pipeline::TRIANGLE_WGSL;

/// Parse one embedded shader constant, panicking with a shader-name-specific
/// message if Naga rejects it.
fn assert_wgsl_parses(name: &str, source: &str) {
    if let Err(err) = naga::front::wgsl::parse_str(source) {
        panic!(
            "embedded WGSL shader `{name}` failed to parse with naga::front::wgsl::parse_str: {err}",
        );
    }
}

#[test]
fn triangle_wgsl_parses() {
    assert_wgsl_parses("TRIANGLE_WGSL", TRIANGLE_WGSL);
}

#[test]
fn mesh_wgsl_parses() {
    assert_wgsl_parses("MESH_WGSL", MESH_WGSL);
}

#[test]
fn lit_mesh_wgsl_parses() {
    assert_wgsl_parses("LIT_MESH_WGSL", LIT_MESH_WGSL);
}
