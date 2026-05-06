//! Token-generation for `#[derive(Reflect)]`.
//!
//! Emits exactly one `impl rge_kernel_types::Reflect for $Self { ... }` block
//! plus a private `const _: () = { ... }` shim for the dynamic field
//! get/set match arms. No global registry, no inventory submission, no
//! helper traits — the generated code is O(fields) tokens.

use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, ToTokens};
use syn::{DeriveInput, Field};

use crate::attrs::{version_expr, ContainerAttrs, DefaultSpec, FieldAttrs, UiHintKind};

pub(crate) fn emit_impl(
    input: &DeriveInput,
    container: &ContainerAttrs,
    fields: &[(&Field, FieldAttrs)],
) -> syn::Result<TokenStream2> {
    let ty_ident = &input.ident;
    let (impl_g, ty_g, where_g) = input.generics.split_for_impl();

    // Crate-path indirection — defaults to ::rge_kernel_types but can be
    // overridden via `#[reflect(crate = "...")]` (used by the kernel-types
    // crate's own internal tests, if any, to avoid the self-dep cycle).
    let krate: TokenStream2 = match &container.crate_path {
        Some(p) => syn::parse_str::<syn::Path>(p)
            .map_err(|e| {
                syn::Error::new_spanned(
                    &input.ident,
                    format!("invalid #[reflect(crate = ...)] path: {e}"),
                )
            })?
            .into_token_stream(),
        None => quote! { ::rge_kernel_types },
    };

    let version_e = version_expr(container, &krate);

    let ty_name_lit = ty_ident.to_string();

    // Build per-field descriptor entries + dynamic match arms.
    let mut descriptor_entries: Vec<TokenStream2> = Vec::with_capacity(fields.len());
    let mut get_arms: Vec<TokenStream2> = Vec::with_capacity(fields.len());
    let mut set_arms: Vec<TokenStream2> = Vec::with_capacity(fields.len());

    for (field, attrs) in fields {
        let field_ident = field.ident.as_ref().expect("named struct field");
        let field_name_lit = field_ident.to_string();
        let ty_str = type_source_text(&field.ty);
        let field_ty = &field.ty;

        let ui_expr = ui_hint_expr(attrs, &krate);
        let range_expr = range_expr(attrs, &krate);
        let default_expr = default_value_expr(attrs, &krate);
        let serde_skip = attrs.skip;

        // Compose the FieldDescriptor literal. The `ty_id` is computed at
        // runtime via TypeId::of_name on the type's source-text spelling.
        // (Phase 2 will switch to <T as Reflect>::TYPE_ID for nested
        // reflected types; primitives remain hashed by name. Note: this
        // means ty_id is a `lazy_static`-style call hidden inside `FIELDS`
        // by way of constructing the slice as an associated `const fn` —
        // but `const` cannot call non-const TypeId::of_name. So Phase 1.1
        // sets ty_id to a zero-hash and relies on `ty_name` for diagnostics.
        // The nested-Reflect path lands in Phase 2.)
        descriptor_entries.push(quote! {
            #krate::FieldDescriptor {
                name:       #field_name_lit,
                ty_name:    #ty_str,
                ty_id:      #krate::TypeId::from_bytes([0u8; 16]),
                range:      #range_expr,
                default:    #default_expr,
                ui_hint:    #ui_expr,
                serde_skip: #serde_skip,
            }
        });

        // Dynamic accessors. Use a span-preserving trick: dispatch by the
        // field type's coercion shape via const-evaluated trait probes.
        // For Phase 1.1 we generate explicit branches for the common
        // primitive types; non-matching types fall through with a
        // TypeMismatch error.
        let getter = field_getter(field_ident, field_ty, &krate);
        let setter = field_setter(field_ident, field_ty, &ty_str, &krate, serde_skip);
        get_arms.push(quote! { #field_name_lit => #getter });
        set_arms.push(quote! { #field_name_lit => #setter });
    }

    let descriptor_count = fields.len();

    let mod_path_concat = quote! {
        ::core::concat!(::core::module_path!(), "::", #ty_name_lit)
    };

    let _ = descriptor_count; // count is implicit in the slice length

    Ok(quote! {
        #[automatically_derived]
        impl #impl_g #krate::Reflect for #ty_ident #ty_g #where_g {
            // RATIONALE: blake3 cannot be `const fn` today (depends on loops).
            // Phase 1.1 declares TYPE_ID as a const filled with zeros; the
            // ReflectObject::type_id() shadow method returns the runtime hash
            // for tooling that needs the live id. Phase 2 will switch to a
            // const-blake3 impl once `blake3` exposes one.
            const TYPE_ID: #krate::TypeId = #krate::TypeId::from_bytes([0u8; 16]);
            const TYPE_NAME: &'static str = #ty_name_lit;
            const FQ_TYPE_NAME: &'static str = #mod_path_concat;
            const SCHEMA_VERSION: #krate::SchemaVersion = #version_e;
            const FIELDS: &'static [#krate::FieldDescriptor] = &[
                #( #descriptor_entries ),*
            ];
            const KIND: #krate::ReflectKind = #krate::ReflectKind::NamedStruct;

            fn get_field_dyn(
                &self,
                __rge_name: &str,
            ) -> ::core::result::Result<#krate::ReflectValue, #krate::ReflectError> {
                // Per-impl helper kept inside the trait fn so multiple
                // #[derive(Reflect)] types in the same module don't collide.
                fn __rge_static_name<R: #krate::Reflect>(name: &str) -> &'static str {
                    for f in R::FIELDS {
                        if f.name == name {
                            return f.name;
                        }
                    }
                    "<unknown>"
                }
                match __rge_name {
                    #( #get_arms , )*
                    other => ::core::result::Result::Err(
                        #krate::ReflectError::UnknownField(__rge_static_name::<Self>(other))
                    ),
                }
            }

            fn set_field_dyn(
                &mut self,
                __rge_name: &str,
                __rge_value: #krate::ReflectValue,
            ) -> ::core::result::Result<(), #krate::ReflectError> {
                fn __rge_static_name<R: #krate::Reflect>(name: &str) -> &'static str {
                    for f in R::FIELDS {
                        if f.name == name {
                            return f.name;
                        }
                    }
                    "<unknown>"
                }
                match __rge_name {
                    #( #set_arms , )*
                    other => ::core::result::Result::Err(
                        #krate::ReflectError::UnknownField(__rge_static_name::<Self>(other))
                    ),
                }
            }
        }
    })
}

/// Render a `syn::Type` back to a compact source-text string. Mirrors the
/// rustforge helper but kept private — we don't want the engine layer to
/// depend on `to_string()` of an arbitrary `Type`.
fn type_source_text(ty: &syn::Type) -> String {
    let s = ty.to_token_stream().to_string();
    // Collapse whitespace runs and drop spaces around `<>,;:&()`.
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !prev_space {
                out.push(' ');
            }
            prev_space = true;
        } else {
            if prev_space && matches!(ch, '<' | '>' | ',' | '(' | ')' | ':' | ';' | '&') {
                out.pop();
            }
            out.push(ch);
            prev_space = false;
        }
    }
    out.trim().to_string()
}

fn ui_hint_expr(attrs: &FieldAttrs, krate: &TokenStream2) -> TokenStream2 {
    match attrs.ui_kind {
        UiHintKind::Default => quote! { #krate::UiHint::Default },
        UiHintKind::Slider => {
            let min = attrs.min.unwrap_or(0.0);
            let max = attrs.max.unwrap_or(1.0);
            let step = attrs.step.unwrap_or(0.0);
            quote! { #krate::UiHint::Slider { min: #min, max: #max, step: #step } }
        }
        UiHintKind::ColorRgb => quote! { #krate::UiHint::ColorRgb },
        UiHintKind::ColorRgba => quote! { #krate::UiHint::ColorRgba },
        UiHintKind::FilePath => {
            let exts: Vec<_> = attrs.extensions.iter().map(|s| quote! { #s }).collect();
            quote! { #krate::UiHint::FilePath { extensions: &[#( #exts ),*] } }
        }
        UiHintKind::EnumDropdown => quote! { #krate::UiHint::EnumDropdown },
        UiHintKind::Multiline => {
            let lines = attrs.lines.unwrap_or(4);
            quote! { #krate::UiHint::Multiline { lines: #lines } }
        }
        UiHintKind::Curve => quote! { #krate::UiHint::Curve },
        UiHintKind::Gradient => quote! { #krate::UiHint::Gradient },
        UiHintKind::Foldout => {
            let open = attrs.default_open.unwrap_or(false);
            quote! { #krate::UiHint::Foldout { default_open: #open } }
        }
        UiHintKind::Inline => quote! { #krate::UiHint::Inline },
        UiHintKind::Hidden => quote! { #krate::UiHint::Hidden },
    }
}

fn range_expr(attrs: &FieldAttrs, krate: &TokenStream2) -> TokenStream2 {
    match (attrs.min, attrs.max) {
        (Some(min), Some(max)) => {
            quote! { ::core::option::Option::Some(#krate::RangeMeta { min: #min, max: #max }) }
        }
        _ => quote! { ::core::option::Option::None },
    }
}

fn default_value_expr(attrs: &FieldAttrs, krate: &TokenStream2) -> TokenStream2 {
    match &attrs.default_expr {
        None => quote! { #krate::DefaultValue::DeriveDefault },
        Some(DefaultSpec::Bool(b)) => quote! { #krate::DefaultValue::Bool(#b) },
        Some(DefaultSpec::Int(i)) => quote! { #krate::DefaultValue::Int(#i) },
        Some(DefaultSpec::Float(f)) => quote! { #krate::DefaultValue::Float(#f) },
        Some(DefaultSpec::String(s)) => {
            quote! { #krate::DefaultValue::String(#s) }
        }
        Some(DefaultSpec::Custom(path)) => {
            quote! { #krate::DefaultValue::Custom(#path) }
        }
    }
}

/// Generate the read arm body for a field.
///
/// The strategy is to dispatch by the field type's source-text spelling
/// and emit `Into<ReflectValue>`-style coercions inline. Phase 1.1 covers
/// the Rust primitives plus `String` / `&str`; everything else lands as
/// `ReflectValue::Unit` with a TODO surfaced via the `ty_name` field.
fn field_getter(
    field_ident: &syn::Ident,
    field_ty: &syn::Type,
    krate: &TokenStream2,
) -> TokenStream2 {
    let ty_str = type_source_text(field_ty);
    match ty_str.as_str() {
        "bool" => quote! {
            ::core::result::Result::Ok(#krate::ReflectValue::Bool(self.#field_ident))
        },
        "i8" | "i16" | "i32" | "i64" | "isize" => quote! {
            ::core::result::Result::Ok(#krate::ReflectValue::I64(self.#field_ident as i64))
        },
        "u8" | "u16" | "u32" | "u64" | "usize" => quote! {
            ::core::result::Result::Ok(#krate::ReflectValue::U64(self.#field_ident as u64))
        },
        "f32" | "f64" => quote! {
            ::core::result::Result::Ok(#krate::ReflectValue::F64(self.#field_ident as f64))
        },
        "String" => quote! {
            ::core::result::Result::Ok(#krate::ReflectValue::String(self.#field_ident.clone()))
        },
        // Fallback: surface as Unit so the reflect walk at least visits.
        // The inspector layer will handle nested-struct cases via a
        // `&dyn ReflectObject` recursion in a later wave.
        _ => quote! {
            ::core::result::Result::Ok(#krate::ReflectValue::Unit)
        },
    }
}

fn field_setter(
    field_ident: &syn::Ident,
    field_ty: &syn::Type,
    field_ty_str: &str,
    krate: &TokenStream2,
    serde_skip: bool,
) -> TokenStream2 {
    if serde_skip {
        let name_lit = field_ident.to_string();
        return quote! {{
            // `#[reflect(skip)]` — refuse the write.
            ::core::result::Result::Err(#krate::ReflectError::SkippedField(#name_lit))
        }};
    }
    let ty_str = type_source_text(field_ty);
    let name_lit = field_ident.to_string();
    let expected_lit = field_ty_str.to_owned();
    match ty_str.as_str() {
        "bool" => quote! {
            match __rge_value {
                #krate::ReflectValue::Bool(b) => { self.#field_ident = b; ::core::result::Result::Ok(()) }
                v => ::core::result::Result::Err(#krate::ReflectError::TypeMismatch {
                    field: #name_lit,
                    expected: #expected_lit,
                    got: v.variant_name(),
                }),
            }
        },
        "i8" | "i16" | "i32" | "i64" | "isize" => quote! {
            match __rge_value {
                #krate::ReflectValue::I64(v) => { self.#field_ident = v as #field_ty; ::core::result::Result::Ok(()) }
                other => ::core::result::Result::Err(#krate::ReflectError::TypeMismatch {
                    field: #name_lit,
                    expected: #expected_lit,
                    got: other.variant_name(),
                }),
            }
        },
        "u8" | "u16" | "u32" | "u64" | "usize" => quote! {
            match __rge_value {
                #krate::ReflectValue::U64(v) => { self.#field_ident = v as #field_ty; ::core::result::Result::Ok(()) }
                other => ::core::result::Result::Err(#krate::ReflectError::TypeMismatch {
                    field: #name_lit,
                    expected: #expected_lit,
                    got: other.variant_name(),
                }),
            }
        },
        "f32" | "f64" => quote! {
            match __rge_value {
                #krate::ReflectValue::F64(v) => { self.#field_ident = v as #field_ty; ::core::result::Result::Ok(()) }
                other => ::core::result::Result::Err(#krate::ReflectError::TypeMismatch {
                    field: #name_lit,
                    expected: #expected_lit,
                    got: other.variant_name(),
                }),
            }
        },
        "String" => quote! {
            match __rge_value {
                #krate::ReflectValue::String(s) => { self.#field_ident = s; ::core::result::Result::Ok(()) }
                #krate::ReflectValue::StaticStr(s) => { self.#field_ident = s.to_owned(); ::core::result::Result::Ok(()) }
                other => ::core::result::Result::Err(#krate::ReflectError::TypeMismatch {
                    field: #name_lit,
                    expected: #expected_lit,
                    got: other.variant_name(),
                }),
            }
        },
        // Fallback — non-primitive, non-String: refuse the write to surface
        // a clear error rather than silently dropping it.
        _ => quote! {
            ::core::result::Result::Err(#krate::ReflectError::TypeMismatch {
                field: #name_lit,
                expected: #expected_lit,
                got: __rge_value.variant_name(),
            })
        },
    }
}
