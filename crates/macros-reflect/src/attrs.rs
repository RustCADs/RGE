//! Attribute parsing for `#[reflect(...)]`.
//!
//! All keys live in this module so the derive entry point in `derive.rs`
//! stays a thin orchestrator. Every attribute is parsed by hand using
//! `syn::Meta::parse_nested_meta` — no `darling`, no `attribute-derive`.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{Attribute, Expr, ExprArray, Lit, Token};

/// Container-level (struct-level) attributes.
#[derive(Debug, Default)]
pub(crate) struct ContainerAttrs {
    /// Optional `version = "x.y.z"`. None falls back to `SchemaVersion::UNVERSIONED`.
    pub(crate) version: Option<(u16, u16, u16)>,

    /// Optional `crate = "..."` — re-routes the kernel-types path. Stored as
    /// the raw string; codegen wraps it in `::path::tokens`.
    pub(crate) crate_path: Option<String>,
}

impl ContainerAttrs {
    pub(crate) fn parse(attrs: &[Attribute]) -> syn::Result<Self> {
        let mut out = ContainerAttrs::default();
        for attr in attrs.iter().filter(|a| a.path().is_ident("reflect")) {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("version") {
                    let lit: Lit = meta.value()?.parse()?;
                    let s = match lit {
                        Lit::Str(s) => s.value(),
                        other => {
                            return Err(syn::Error::new_spanned(
                                other,
                                "#[reflect(version = ...)] expects a string literal like \"1.0.0\"",
                            ));
                        }
                    };
                    out.version = Some(parse_version(&s).map_err(|e| {
                        syn::Error::new_spanned(&meta.path, format!("invalid version string: {e}"))
                    })?);
                } else if meta.path.is_ident("crate") {
                    let lit: Lit = meta.value()?.parse()?;
                    let s = match lit {
                        Lit::Str(s) => s.value(),
                        other => {
                            return Err(syn::Error::new_spanned(
                                other,
                                "#[reflect(crate = ...)] expects a string literal",
                            ));
                        }
                    };
                    out.crate_path = Some(s);
                } else {
                    return Err(meta.error(
                        "unknown container-level #[reflect(...)] key (expected: version | crate)",
                    ));
                }
                Ok(())
            })?;
        }
        Ok(out)
    }
}

fn parse_version(s: &str) -> Result<(u16, u16, u16), String> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 3 {
        return Err(format!("expected `major.minor.patch`, got `{s}`"));
    }
    let major: u16 = parts[0].parse().map_err(|e| format!("major: {e}"))?;
    let minor: u16 = parts[1].parse().map_err(|e| format!("minor: {e}"))?;
    let patch: u16 = parts[2].parse().map_err(|e| format!("patch: {e}"))?;
    Ok((major, minor, patch))
}

/// UI hint kind requested by `#[reflect(ui = "...")]`. Maps to `UiHint`
/// variants on the kernel side. Kept as an enum here so the macro can match
/// on the kind without round-tripping through strings during codegen.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub(crate) enum UiHintKind {
    #[default]
    Default,
    Slider,
    ColorRgb,
    ColorRgba,
    FilePath,
    EnumDropdown,
    Multiline,
    Curve,
    Gradient,
    Foldout,
    Inline,
    Hidden,
}

impl UiHintKind {
    fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "Default" => Self::Default,
            "Slider" => Self::Slider,
            "ColorRgb" => Self::ColorRgb,
            "ColorRgba" => Self::ColorRgba,
            "FilePath" => Self::FilePath,
            "EnumDropdown" => Self::EnumDropdown,
            "Multiline" => Self::Multiline,
            "Curve" => Self::Curve,
            "Gradient" => Self::Gradient,
            "Foldout" => Self::Foldout,
            "Inline" => Self::Inline,
            "Hidden" => Self::Hidden,
            _ => return None,
        })
    }
}

/// Field-level attributes after parsing.
#[derive(Debug, Default)]
pub(crate) struct FieldAttrs {
    pub(crate) skip: bool,
    pub(crate) ui_kind: UiHintKind,
    pub(crate) min: Option<f64>,
    pub(crate) max: Option<f64>,
    pub(crate) step: Option<f64>,
    pub(crate) extensions: Vec<String>,
    pub(crate) lines: Option<u16>,
    pub(crate) default_open: Option<bool>,
    pub(crate) validate: Option<String>,
    pub(crate) custom_drawer: Option<String>,
    pub(crate) default_expr: Option<DefaultSpec>,
}

#[derive(Debug)]
pub(crate) enum DefaultSpec {
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    /// Path to `fn() -> T`.
    Custom(String),
}

impl FieldAttrs {
    pub(crate) fn parse(attrs: &[Attribute]) -> syn::Result<Self> {
        let mut out = FieldAttrs::default();
        for attr in attrs.iter().filter(|a| a.path().is_ident("reflect")) {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("skip") {
                    out.skip = true;
                } else if meta.path.is_ident("ui") {
                    let lit: Lit = meta.value()?.parse()?;
                    let s = expect_str(&lit, "#[reflect(ui = ...)] expects a string literal")?;
                    out.ui_kind = UiHintKind::from_str(&s).ok_or_else(|| {
                        syn::Error::new_spanned(
                            &lit,
                            format!("unknown UiHint variant `{s}` — see PLAN.md §6.15 closed set"),
                        )
                    })?;
                } else if meta.path.is_ident("min") {
                    let lit: Lit = meta.value()?.parse()?;
                    out.min = Some(numeric_lit_to_f64(&lit)?);
                } else if meta.path.is_ident("max") {
                    let lit: Lit = meta.value()?.parse()?;
                    out.max = Some(numeric_lit_to_f64(&lit)?);
                } else if meta.path.is_ident("step") {
                    let lit: Lit = meta.value()?.parse()?;
                    out.step = Some(numeric_lit_to_f64(&lit)?);
                } else if meta.path.is_ident("extensions") {
                    let value = meta.value()?;
                    let arr: ExprArray = value.parse()?;
                    let mut exts = Vec::with_capacity(arr.elems.len());
                    for el in &arr.elems {
                        if let Expr::Lit(lit) = el {
                            exts.push(expect_str(
                                &lit.lit,
                                "#[reflect(extensions = [...])] expects string literals",
                            )?);
                        } else {
                            return Err(syn::Error::new_spanned(
                                el,
                                "expected string literal in extensions array",
                            ));
                        }
                    }
                    out.extensions = exts;
                } else if meta.path.is_ident("lines") {
                    let lit: Lit = meta.value()?.parse()?;
                    let n: u64 = match &lit {
                        Lit::Int(i) => i.base10_parse()?,
                        other => {
                            return Err(syn::Error::new_spanned(
                                other,
                                "#[reflect(lines = N)] expects an integer literal",
                            ));
                        }
                    };
                    out.lines = Some(
                        u16::try_from(n)
                            .map_err(|_| syn::Error::new_spanned(&lit, "lines must fit in u16"))?,
                    );
                } else if meta.path.is_ident("default_open") {
                    let lit: Lit = meta.value()?.parse()?;
                    let b = match lit {
                        Lit::Bool(b) => b.value,
                        other => {
                            return Err(syn::Error::new_spanned(
                                other,
                                "#[reflect(default_open = ...)] expects a bool literal",
                            ));
                        }
                    };
                    out.default_open = Some(b);
                } else if meta.path.is_ident("validate") {
                    let lit: Lit = meta.value()?.parse()?;
                    out.validate = Some(expect_str(
                        &lit,
                        "#[reflect(validate = ...)] expects a string literal symbol path",
                    )?);
                } else if meta.path.is_ident("custom_drawer") {
                    let lit: Lit = meta.value()?.parse()?;
                    out.custom_drawer = Some(expect_str(
                        &lit,
                        "#[reflect(custom_drawer = ...)] expects a string literal symbol path",
                    )?);
                } else if meta.path.is_ident("default") {
                    let lit: Lit = meta.value()?.parse()?;
                    out.default_expr = Some(default_from_lit(&lit)?);
                } else if meta.path.is_ident("default_with") {
                    let lit: Lit = meta.value()?.parse()?;
                    out.default_expr = Some(DefaultSpec::Custom(expect_str(
                        &lit,
                        "#[reflect(default_with = ...)] expects a string literal symbol path",
                    )?));
                } else {
                    let key = meta
                        .path
                        .get_ident()
                        .map(ToString::to_string)
                        .unwrap_or_else(|| "<path>".into());
                    return Err(meta.error(format!(
                        "unknown field-level #[reflect({key} ...)] key — \
                         allowed: skip | ui | min | max | step | extensions | lines | \
                         default_open | validate | custom_drawer | default | default_with"
                    )));
                }
                Ok(())
            })?;
        }
        // Sanity: ui = Slider requires min and max.
        if out.ui_kind == UiHintKind::Slider && (out.min.is_none() || out.max.is_none()) {
            // Soft error — emit a clear span at the first reflect attr.
            return Err(syn::Error::new(
                attrs
                    .iter()
                    .find(|a| a.path().is_ident("reflect"))
                    .map(|a| a.span())
                    .unwrap_or_else(proc_macro2::Span::call_site),
                "#[reflect(ui = \"Slider\")] requires both min and max",
            ));
        }
        Ok(out)
    }
}

fn expect_str(lit: &Lit, msg: &'static str) -> syn::Result<String> {
    match lit {
        Lit::Str(s) => Ok(s.value()),
        other => Err(syn::Error::new_spanned(other, msg)),
    }
}

fn numeric_lit_to_f64(lit: &Lit) -> syn::Result<f64> {
    match lit {
        Lit::Float(f) => f.base10_parse::<f64>(),
        Lit::Int(i) => i.base10_parse::<f64>(),
        other => Err(syn::Error::new_spanned(
            other,
            "expected numeric literal (e.g. 0.5 or 42)",
        )),
    }
}

fn default_from_lit(lit: &Lit) -> syn::Result<DefaultSpec> {
    Ok(match lit {
        Lit::Bool(b) => DefaultSpec::Bool(b.value),
        Lit::Int(i) => DefaultSpec::Int(i.base10_parse::<i64>()?),
        Lit::Float(f) => DefaultSpec::Float(f.base10_parse::<f64>()?),
        Lit::Str(s) => DefaultSpec::String(s.value()),
        other => {
            return Err(syn::Error::new_spanned(
                other,
                "#[reflect(default = ...)] expects a literal (bool / int / float / string)",
            ));
        }
    })
}

/// Consumed only by the codegen module. Held here so the only place
/// `Punctuated`/`Token` are referenced is `attrs.rs`.
#[allow(dead_code)]
type ParenLit = Punctuated<Lit, Token![,]>;

/// Render the parsed [`ContainerAttrs::version`] into a `SchemaVersion::new(...)`
/// expression. Lifted up so codegen need not know the triple's shape.
pub(crate) fn version_expr(c: &ContainerAttrs, krate: &TokenStream2) -> TokenStream2 {
    match c.version {
        Some((mj, mi, pa)) => quote! { #krate::SchemaVersion::new(#mj, #mi, #pa) },
        None => quote! { #krate::SchemaVersion::UNVERSIONED },
    }
}
