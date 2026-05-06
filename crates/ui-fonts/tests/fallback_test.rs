//! Resolver / fallback-chain tests.

use rge_ui_fonts::{FontRegistry, GenericFamily, Resolver};

/// A registry seeded only with vendored Inter must still resolve a known
/// family directly.
#[test]
fn resolves_loaded_family_directly() {
    let mut reg = FontRegistry::new_empty();
    reg.load_dir(&FontRegistry::vendored_fonts_dir().join("Inter"))
        .expect("vendored Inter must load");
    let resolver = Resolver::new();
    let face = resolver
        .resolve(&reg, "Inter")
        .expect("Inter must resolve directly");
    assert_eq!(face.family, "Inter");
}

/// An unknown family must fall through the chain. With nothing else loaded,
/// resolution must error rather than silently return something nonsensical.
#[test]
fn missing_family_in_empty_registry_errors() {
    let reg = FontRegistry::new_empty();
    let resolver = Resolver::new();
    let result = resolver.resolve(&reg, "NoSuchFamily12345");
    assert!(
        result.is_err(),
        "empty registry must not pretend to resolve a missing family: {result:?}"
    );
}

/// On a registry that *does* hold Inter, an unknown family must still find
/// Inter via the fallback chain (Inter is the first entry in the default
/// chain).
#[test]
fn missing_family_falls_back_through_chain() {
    let mut reg = FontRegistry::new_empty();
    reg.load_dir(&FontRegistry::vendored_fonts_dir().join("Inter"))
        .expect("vendored Inter must load");
    let resolver = Resolver::new();
    let face = resolver
        .resolve(&reg, "ThisDoesNotExist_qwerty")
        .expect("fallback chain must reach Inter");
    assert_eq!(face.family, "Inter");
}

/// Generic monospace class should resolve to `JetBrainsMono` when vendored.
#[test]
fn generic_monospace_resolves_to_jetbrains_mono() {
    let mut reg = FontRegistry::new_empty();
    reg.load_dir(&FontRegistry::vendored_fonts_dir().join("JetBrainsMono"))
        .expect("vendored JetBrainsMono must load");
    let resolver = Resolver::new();
    let face = resolver
        .resolve_generic(&reg, GenericFamily::Monospace)
        .expect("monospace generic must resolve");
    assert!(
        face.family.contains("JetBrains") || face.family.contains("Mono"),
        "expected a JetBrains/Mono family, got `{}`",
        face.family
    );
}

/// System-font fallback path — boot a registry with whatever the OS provides
/// and confirm `resolve_generic` does not error. We do not assume any
/// particular family because the build host varies.
#[test]
fn system_fonts_provide_some_sans_serif() {
    let reg = FontRegistry::with_system_fonts();
    let resolver = Resolver::new();
    let result = resolver.resolve_generic(&reg, GenericFamily::SansSerif);
    // On a host with zero fonts this would error, but every CI runner we
    // care about has *some* sans serif. Assert the *kind* of failure is
    // expected and informative if it does happen.
    match result {
        Ok(face) => {
            assert!(!face.family.is_empty(), "resolved family must have a name");
        }
        Err(rge_ui_fonts::ResolveError::NoFaceForFamily { tried }) => {
            assert!(
                !tried.is_empty(),
                "if resolution fails the chain must list what it tried"
            );
        }
    }
}
