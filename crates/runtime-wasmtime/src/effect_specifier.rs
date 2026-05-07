// adapted from rustforge::crates::runtime-wasmtime on 2026-05-05 — engine_wasmtime feature activated
//! Verse-style effect specifiers for WASM Component-Model plugins.
//!
//! Each plugin's WIT-typed entry point is annotated (in source comments
//! parsed at registration) with one or more **effects** drawn from a
//! closed enumeration:
//!
//! - `<computes>`  — pure read-only data transformation
//! - `<varies>`    — non-determinism allowed (RNG, system time)
//! - `<transacts>` — may mutate IR / side-effect state-tree
//! - `<reads-phi>` — touches Protected Health Information (HIPAA / GDPR)
//! - `<network>`   — initiates outbound network requests
//!
//! Each effect maps to one or more **capability codes** the host must
//! grant before the plugin may instantiate. The map is intentionally
//! **non-monotone**: granting `<transacts>` does NOT imply
//! `<computes>` — every effect a plugin actually exercises must be
//! declared.
//!
//! The [`CapTicket<C>`] type is parameterised over a const-generic
//! capability bitmask. The [`Plugin<E>`] type is parameterised over a
//! const-generic effect bitmask. A plugin can only [`Plugin::bind`]
//! against a ticket whose capability bits are a **superset** of the
//! plugin's effect's required capabilities. The check is `const fn`
//! and surfaces as a `static_assert!` failure at type-check time.
//!
//! For **WASM dynamic plugins** loaded from disk at editor runtime,
//! the const-generic gate cannot apply (the compiler doesn't see the
//! bitmask). The runtime [`grant_check`] helper plus
//! [`crate::runtime::WasmRuntime::instantiate`] cover that path with
//! the same predicate fired at instantiation time.

use core::marker::PhantomData;

// ---------------------------------------------------------------------------
// Effect enum + bitmask helpers
// ---------------------------------------------------------------------------

/// One of the five Verse-style effect specifiers a plugin may declare.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Effect {
    /// Pure read-only data transformation (no I/O, no mutation).
    Computes,
    /// Non-determinism allowed (RNG, wall-clock time, system entropy).
    Varies,
    /// May mutate IR or state-tree on the host side.
    Transacts,
    /// Touches Protected Health Information.
    ReadsPhi,
    /// Initiates outbound network requests.
    Network,
}

impl Effect {
    /// Tag literal as it appears in WIT source comments
    /// (e.g. `<computes>`, `<reads-phi>`).
    #[must_use]
    pub fn as_tag(self) -> &'static str {
        match self {
            Effect::Computes => "<computes>",
            Effect::Varies => "<varies>",
            Effect::Transacts => "<transacts>",
            Effect::ReadsPhi => "<reads-phi>",
            Effect::Network => "<network>",
        }
    }

    /// Single-bit position in the const-generic effect bitmask.
    #[must_use]
    pub const fn bit(self) -> u32 {
        match self {
            Effect::Computes => 1 << 0,
            Effect::Varies => 1 << 1,
            Effect::Transacts => 1 << 2,
            Effect::ReadsPhi => 1 << 3,
            Effect::Network => 1 << 4,
        }
    }

    /// Capability codes required by hosts to permit this effect.
    #[must_use]
    pub const fn required_caps(self) -> u32 {
        match self {
            Effect::Computes => Capability::ComputeExec.bit(),
            Effect::Varies => Capability::ComputeExec.bit() | Capability::EntropyRead.bit(),
            Effect::Transacts => Capability::ComputeExec.bit() | Capability::IrWrite.bit(),
            Effect::ReadsPhi => Capability::ComputeExec.bit() | Capability::PhiRead.bit(),
            Effect::Network => Capability::ComputeExec.bit() | Capability::NetworkOutbound.bit(),
        }
    }

    /// Round-trip the WIT tag literal back to an `Effect`.
    #[must_use]
    pub fn from_tag(s: &str) -> Option<Self> {
        match s {
            "<computes>" => Some(Effect::Computes),
            "<varies>" => Some(Effect::Varies),
            "<transacts>" => Some(Effect::Transacts),
            "<reads-phi>" => Some(Effect::ReadsPhi),
            "<network>" => Some(Effect::Network),
            _ => None,
        }
    }

    /// Iterate over all five effects in declaration order.
    #[must_use]
    pub const fn all() -> [Effect; 5] {
        [
            Effect::Computes,
            Effect::Varies,
            Effect::Transacts,
            Effect::ReadsPhi,
            Effect::Network,
        ]
    }
}

/// Set of effects a plugin declares it may exercise. Stored as a
/// `u32` bitmask so the const-generic [`Plugin`] can use it
/// in type position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct EffectSet(pub u32);

impl EffectSet {
    /// Empty set — pure plugin (probably useless; allowed for completeness).
    pub const EMPTY: EffectSet = EffectSet(0);

    /// Builder: set with a single effect.
    #[must_use]
    pub const fn from_one(e: Effect) -> Self {
        EffectSet(e.bit())
    }

    /// Builder: union of two sets.
    #[must_use]
    pub const fn union(self, other: EffectSet) -> Self {
        EffectSet(self.0 | other.0)
    }

    /// Builder: add a single effect.
    #[must_use]
    pub const fn with(self, e: Effect) -> Self {
        EffectSet(self.0 | e.bit())
    }

    /// True when this set contains the given effect.
    #[must_use]
    pub const fn contains(self, e: Effect) -> bool {
        (self.0 & e.bit()) != 0
    }

    /// Capability bitmask the host must grant for this set to instantiate.
    #[must_use]
    pub const fn required_caps(self) -> u32 {
        let mut acc = 0u32;
        let effects = Effect::all();
        let mut i = 0;
        while i < effects.len() {
            let e = effects[i];
            if (self.0 & e.bit()) != 0 {
                acc |= e.required_caps();
            }
            i += 1;
        }
        acc
    }

    /// Number of effects declared.
    #[must_use]
    pub const fn count(self) -> u32 {
        self.0.count_ones()
    }

    /// Iterate over the effects in this set.
    pub fn iter(self) -> impl Iterator<Item = Effect> {
        Effect::all().into_iter().filter(move |e| self.contains(*e))
    }
}

// ---------------------------------------------------------------------------
// Capability enum + bitmask helpers
// ---------------------------------------------------------------------------

/// Host-granted capabilities. Each maps to an exclusive bit in the
/// capability bitmask of a [`CapTicket`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Capability {
    /// "May execute any plugin code at all." Base cap — every effect
    /// requires this.
    ComputeExec,
    /// May read system entropy / wall-clock (for `<varies>`).
    EntropyRead,
    /// May call IR-mutating host functions (for `<transacts>`).
    IrWrite,
    /// May read patient PHI fields (for `<reads-phi>`).
    PhiRead,
    /// May initiate outbound network requests (for `<network>`).
    NetworkOutbound,
}

impl Capability {
    /// Single-bit position in the capability bitmask.
    #[must_use]
    pub const fn bit(self) -> u32 {
        match self {
            Capability::ComputeExec => 1 << 0,
            Capability::EntropyRead => 1 << 1,
            Capability::IrWrite => 1 << 2,
            Capability::PhiRead => 1 << 3,
            Capability::NetworkOutbound => 1 << 4,
        }
    }

    /// String code as used in `<reads-phi>` ↔ `phi.read` mapping.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Capability::ComputeExec => "compute.exec",
            Capability::EntropyRead => "entropy.read",
            Capability::IrWrite => "ir.write",
            Capability::PhiRead => "phi.read",
            Capability::NetworkOutbound => "network.outbound",
        }
    }

    /// All known capability codes (declaration order).
    #[must_use]
    pub const fn all() -> [Capability; 5] {
        [
            Capability::ComputeExec,
            Capability::EntropyRead,
            Capability::IrWrite,
            Capability::PhiRead,
            Capability::NetworkOutbound,
        ]
    }

    /// Inverse of [`code`]; round-trip parser.
    #[must_use]
    pub fn from_code(s: &str) -> Option<Self> {
        match s {
            "compute.exec" => Some(Capability::ComputeExec),
            "entropy.read" => Some(Capability::EntropyRead),
            "ir.write" => Some(Capability::IrWrite),
            "phi.read" => Some(Capability::PhiRead),
            "network.outbound" => Some(Capability::NetworkOutbound),
            _ => None,
        }
    }
}

/// Bag-of-capabilities an issuing authority has granted to a host
/// session. Concrete `CapTicket<CAPS>` is the typestate — a plugin
/// generic over `EFFECTS` can only bind against tickets whose `CAPS`
/// is a superset of the effect set's required caps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct CapSet(pub u32);

impl CapSet {
    /// Empty grant.
    pub const EMPTY: CapSet = CapSet(0);

    /// Grant exactly one capability.
    #[must_use]
    pub const fn from_one(c: Capability) -> Self {
        CapSet(c.bit())
    }

    /// Grant the union of two cap sets.
    #[must_use]
    pub const fn union(self, other: CapSet) -> Self {
        CapSet(self.0 | other.0)
    }

    /// Add a single cap.
    #[must_use]
    pub const fn with(self, c: Capability) -> Self {
        CapSet(self.0 | c.bit())
    }

    /// True when this set contains the given cap.
    #[must_use]
    pub const fn contains(self, c: Capability) -> bool {
        (self.0 & c.bit()) != 0
    }

    /// True iff this cap set is a superset of `required` — every bit in
    /// `required` is also set here.
    #[must_use]
    pub const fn is_superset_of(self, required: u32) -> bool {
        (self.0 & required) == required
    }

    /// All-five-caps "developer" grant. Used in tests; production
    /// code should always whitelist.
    #[must_use]
    pub const fn all() -> Self {
        CapSet(
            Capability::ComputeExec.bit()
                | Capability::EntropyRead.bit()
                | Capability::IrWrite.bit()
                | Capability::PhiRead.bit()
                | Capability::NetworkOutbound.bit(),
        )
    }

    /// Iterate the capabilities in this set.
    pub fn iter(self) -> impl Iterator<Item = Capability> {
        Capability::all()
            .into_iter()
            .filter(move |c| self.contains(*c))
    }
}

// ---------------------------------------------------------------------------
// Sealed CapTicket marker — typestate for compile-time gate
// ---------------------------------------------------------------------------

mod sealed {
    /// Sealed trait — only [`super::CapTicket`] may implement
    /// [`super::CapMarker`].
    pub trait Sealed {}
}

/// Witness that a const-generic `CAPS: u32` is a valid capability
/// bitmask. Sealed: downstream crates cannot smuggle in their own
/// markers and bypass the type-system guarantee.
pub trait CapMarker: sealed::Sealed + Copy + Clone + 'static {
    /// Capability bitmask this marker certifies.
    const CAPS: u32;
}

/// Type-level capability ticket. The const-generic `CAPS` parameter
/// records exactly which capabilities the issuing authority granted.
#[derive(Debug, Clone, Copy)]
pub struct CapTicket<const CAPS: u32> {
    /// Plugin-id this ticket was issued to (audit log key).
    pub plugin_id: blake3::Hash,
    /// Issuing authority — typically the host's session id; opaque to plugin.
    pub issuer: &'static str,
    /// Marker so the unused `CAPS` parameter satisfies coherence.
    _marker: PhantomData<()>,
}

impl<const CAPS: u32> CapTicket<CAPS> {
    /// Construct a ticket carrying the capabilities encoded in `CAPS`.
    #[must_use]
    pub const fn new(plugin_id: blake3::Hash, issuer: &'static str) -> Self {
        Self {
            plugin_id,
            issuer,
            _marker: PhantomData,
        }
    }

    /// Cap bitmask carried by this ticket (compile-time const).
    pub const CAPS: u32 = CAPS;
}

impl<const CAPS: u32> sealed::Sealed for CapTicket<CAPS> {}
impl<const CAPS: u32> CapMarker for CapTicket<CAPS> {
    const CAPS: u32 = CAPS;
}

// ---------------------------------------------------------------------------
// Compile-time gate: const fn satisfies(...)
// ---------------------------------------------------------------------------

/// Const-fn predicate: does ticket-cap-bitmask `caps` satisfy
/// effect-set-required-caps `required`?
#[must_use]
pub const fn cap_set_satisfies(caps: u32, required: u32) -> bool {
    (caps & required) == required
}

/// Runtime-side mirror of [`cap_set_satisfies`]. Returns a structured
/// error so the host can `tracing::warn!` the rejection and surface a
/// human-readable reason.
///
/// # Errors
/// Returns `GrantError::MissingCapability` if `granted` does not cover
/// every capability required by `plugin_effects`.
pub fn grant_check(plugin_effects: EffectSet, granted: CapSet) -> Result<(), GrantError> {
    let required = plugin_effects.required_caps();
    if granted.is_superset_of(required) {
        Ok(())
    } else {
        let missing_bits = required & !granted.0;
        let missing = Capability::all()
            .into_iter()
            .find(|c| (missing_bits & c.bit()) != 0);
        Err(GrantError::MissingCapability {
            effects: plugin_effects,
            missing,
            missing_bits,
        })
    }
}

/// Reason a capability ticket failed to satisfy a plugin's effect set.
#[derive(Debug, thiserror::Error)]
pub enum GrantError {
    /// Plugin declared an effect whose required capability was not granted.
    #[error(
        "plugin declared effects {effects:?} require caps not granted; missing first = {missing:?} (bits = 0b{missing_bits:b})"
    )]
    MissingCapability {
        /// Plugin's full declared effect set.
        effects: EffectSet,
        /// First missing cap (declaration-order); `None` only if `missing_bits == 0`.
        missing: Option<Capability>,
        /// Raw bitmask of all missing caps — useful for audit logs.
        missing_bits: u32,
    },
}

// ---------------------------------------------------------------------------
// Plugin<EFFECTS> — phantom type carrying a plugin's effect declaration
// ---------------------------------------------------------------------------

/// Type-level plugin handle, parameterised over the const-generic
/// effect bitmask.
#[derive(Debug, Clone, Copy)]
pub struct Plugin<const EFFECTS: u32> {
    /// blake3 of the .wasm component bytes — the audit-log key.
    pub plugin_id: blake3::Hash,
    /// Source-level plugin name (informational).
    pub name: &'static str,
}

impl<const EFFECTS: u32> Plugin<EFFECTS> {
    /// Construct a plugin handle.
    #[must_use]
    pub const fn new(plugin_id: blake3::Hash, name: &'static str) -> Self {
        Self { plugin_id, name }
    }

    /// Effect bitmask carried by this plugin (compile-time const).
    pub const EFFECTS: u32 = EFFECTS;

    /// **The compile-time gate.** Bind a plugin to a capability ticket
    /// whose const-generic `CAPS` is a superset of this plugin's
    /// `EFFECTS.required_caps()`.
    #[must_use]
    pub const fn bind<const CAPS: u32>(
        self,
        ticket: CapTicket<CAPS>,
    ) -> BoundPlugin<EFFECTS, CAPS> {
        BoundPlugin {
            plugin_id: self.plugin_id,
            name: self.name,
            ticket,
        }
    }
}

/// A plugin successfully bound to a capability ticket.
#[derive(Debug, Clone, Copy)]
pub struct BoundPlugin<const EFFECTS: u32, const CAPS: u32> {
    /// blake3 of the .wasm component bytes.
    pub plugin_id: blake3::Hash,
    /// Source-level plugin name.
    pub name: &'static str,
    /// The ticket this plugin is bound under.
    pub ticket: CapTicket<CAPS>,
}

/// Compile-time assertion macro the **call site** uses to enforce
/// the EFFECTS-vs-CAPS gate.
#[macro_export]
macro_rules! assert_compile_time_gate {
    ($effects:expr, $caps:expr) => {{
        const _GATE: () = {
            let required = $crate::effect_specifier::EffectSet($effects).required_caps();
            assert!(
                $crate::effect_specifier::cap_set_satisfies($caps, required),
                "plugin EFFECTS require capabilities not granted by ticket CAPS"
            );
        };
        _GATE
    }};
}

/// Const-fn predicate: is `callee` effect bitmask a **subset** of
/// `caller` effect bitmask?
#[must_use]
#[allow(
    clippy::similar_names,
    reason = "`caller` / `callee` is the canonical pair for control-flow effect propagation; renaming would diverge from the surrounding §1.13 doctrine and the matching parameter names in linker.rs"
)]
pub const fn effect_set_subset(caller: u32, callee: u32) -> bool {
    (caller & callee) == callee
}

/// Build a `u32` effect bitmask from a comma-list of bare-ident effect tags.
#[macro_export]
macro_rules! effects_mask {
    () => { 0u32 };
    ( $($eff:ident),+ $(,)? ) => {
        0u32 $( | $crate::effects_mask!(@one $eff) )+
    };
    (@one computes)  => { $crate::effect_specifier::Effect::Computes.bit() };
    (@one varies)    => { $crate::effect_specifier::Effect::Varies.bit() };
    (@one transacts) => { $crate::effect_specifier::Effect::Transacts.bit() };
    (@one reads_phi) => { $crate::effect_specifier::Effect::ReadsPhi.bit() };
    (@one network)   => { $crate::effect_specifier::Effect::Network.bit() };
}

/// Compile-time assertion macro the **call site** uses to enforce the
/// callee-effects ⊆ caller-effects gate.
#[macro_export]
macro_rules! assert_effect_subset {
    ($caller:expr, $callee:expr $(,)?) => {{
        const _SUBSET_GATE: () = {
            assert!(
                $crate::effect_specifier::effect_set_subset($caller, $callee),
                "callee EFFECTS are not a subset of caller EFFECTS — \
                 declare every callee effect on the caller (or refuse to call)"
            );
        };
        _SUBSET_GATE
    }};
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effect_tag_round_trip() {
        for e in Effect::all() {
            assert_eq!(Effect::from_tag(e.as_tag()), Some(e));
        }
        assert_eq!(Effect::from_tag("<undeclared>"), None);
    }

    #[test]
    fn effect_bits_disjoint() {
        let bits: Vec<u32> = Effect::all().into_iter().map(super::Effect::bit).collect();
        for (i, &a) in bits.iter().enumerate() {
            for (j, &b) in bits.iter().enumerate() {
                if i != j {
                    assert_eq!(a & b, 0, "effect bits {i} and {j} overlap");
                }
            }
        }
    }

    #[test]
    fn capability_code_round_trip() {
        for c in Capability::all() {
            assert_eq!(Capability::from_code(c.code()), Some(c));
        }
        assert_eq!(Capability::from_code("phi.write"), None);
    }

    #[test]
    fn effect_required_caps_correct() {
        let req = Effect::ReadsPhi.required_caps();
        assert!(req & Capability::PhiRead.bit() != 0);
        assert!(req & Capability::ComputeExec.bit() != 0);
        assert!(req & Capability::IrWrite.bit() == 0);
        assert!(req & Capability::NetworkOutbound.bit() == 0);
    }

    #[test]
    fn cap_set_superset_check() {
        let granted = CapSet::from_one(Capability::ComputeExec).with(Capability::PhiRead);
        let needed_phi_only = Effect::ReadsPhi.required_caps();
        assert!(granted.is_superset_of(needed_phi_only));

        let granted2 = CapSet::from_one(Capability::ComputeExec);
        assert!(!granted2.is_superset_of(needed_phi_only));
    }

    #[test]
    fn grant_check_rejects_missing_phi_read() {
        let effects = EffectSet::from_one(Effect::ReadsPhi);
        let granted = CapSet::from_one(Capability::ComputeExec);
        let r = grant_check(effects, granted);
        match r {
            Err(GrantError::MissingCapability { missing, .. }) => {
                assert_eq!(missing, Some(Capability::PhiRead));
            }
            other => panic!("expected MissingCapability, got {other:?}"),
        }
    }

    #[test]
    fn grant_check_accepts_complete_grant() {
        let effects = EffectSet::from_one(Effect::ReadsPhi)
            .with(Effect::Computes)
            .with(Effect::Transacts);
        let granted = CapSet::all();
        assert!(grant_check(effects, granted).is_ok());
    }

    #[test]
    fn grant_check_rejects_network_without_grant() {
        let effects = EffectSet::from_one(Effect::Network);
        let granted = CapSet::from_one(Capability::ComputeExec);
        let r = grant_check(effects, granted);
        assert!(matches!(
            r,
            Err(GrantError::MissingCapability {
                missing: Some(Capability::NetworkOutbound),
                ..
            })
        ));
    }

    #[test]
    fn empty_effect_set_satisfied_by_empty_grant() {
        let effects = EffectSet::EMPTY;
        let granted = CapSet::EMPTY;
        assert!(grant_check(effects, granted).is_ok());
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn cap_set_satisfies_const_fn_in_const_context() {
        const REQ: u32 = Effect::ReadsPhi.required_caps();
        const HAVE: u32 = CapSet::all().0;
        const OK: bool = cap_set_satisfies(HAVE, REQ);
        assert!(OK);
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn const_fn_cap_satisfies_rejects_missing() {
        const REQ: u32 = Effect::ReadsPhi.required_caps();
        const HAVE: u32 = CapSet::from_one(Capability::ComputeExec).0;
        const OK: bool = cap_set_satisfies(HAVE, REQ);
        assert!(!OK);
    }

    #[test]
    fn cap_ticket_sealed_const_caps_round_trip() {
        const CAPS: u32 = CapSet::all().0;
        let h = blake3::hash(b"test-plugin");
        let ticket: CapTicket<CAPS> = CapTicket::new(h, "test-issuer");
        assert_eq!(ticket.plugin_id, h);
        assert_eq!(ticket.issuer, "test-issuer");
        assert_eq!(<CapTicket<CAPS> as CapMarker>::CAPS, CAPS);
    }

    #[test]
    fn assert_compile_time_gate_macro_compiles_when_caps_sufficient() {
        const PLUGIN_EFFECTS: u32 = EffectSet::from_one(Effect::ReadsPhi).0;
        const HOST_CAPS: u32 = CapSet::from_one(Capability::ComputeExec)
            .with(Capability::PhiRead)
            .0;
        crate::assert_compile_time_gate!(PLUGIN_EFFECTS, HOST_CAPS);
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn effect_set_subset_const_fn_holds_for_superset() {
        const CALLER: u32 = crate::effects_mask!(reads_phi, computes);
        const CALLEE: u32 = crate::effects_mask!(reads_phi);
        const OK: bool = effect_set_subset(CALLER, CALLEE);
        assert!(OK);
    }

    #[test]
    fn effects_mask_empty_is_zero() {
        const EMPTY: u32 = crate::effects_mask!();
        assert_eq!(EMPTY, 0);
    }

    #[test]
    fn effects_mask_multi_combines_bits() {
        const M: u32 = crate::effects_mask!(computes, reads_phi, network);
        assert_eq!(
            M,
            Effect::Computes.bit() | Effect::ReadsPhi.bit() | Effect::Network.bit(),
        );
    }
}
