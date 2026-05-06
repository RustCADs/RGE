//! Top-level orchestrator for `#[derive(Reflect)]`.
//!
//! Splits into:
//! 1. Parse `DeriveInput`.
//! 2. Validate shape (Phase 1.1: structs with named fields only).
//! 3. Parse container + per-field attributes via [`crate::attrs`].
//! 4. Hand off to [`crate::codegen`] for token emission.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use syn::{parse_macro_input, Data, DataStruct, DeriveInput, Fields, FieldsNamed};

use crate::attrs::{ContainerAttrs, FieldAttrs};
use crate::codegen;

pub(crate) fn expand(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match try_expand(&input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn try_expand(input: &DeriveInput) -> syn::Result<TokenStream2> {
    let container = ContainerAttrs::parse(&input.attrs)?;

    let fields = match &input.data {
        Data::Struct(DataStruct {
            fields: Fields::Named(FieldsNamed { named, .. }),
            ..
        }) => named,
        Data::Struct(_) => {
            return Err(syn::Error::new_spanned(
                &input.ident,
                "#[derive(Reflect)] requires a struct with named fields. \
                 Tuple structs and unit structs will be supported in a later phase.",
            ));
        }
        Data::Enum(_) => {
            return Err(syn::Error::new_spanned(
                &input.ident,
                "#[derive(Reflect)] does not yet support enums (Phase 2 deliverable).",
            ));
        }
        Data::Union(_) => {
            return Err(syn::Error::new_spanned(
                &input.ident,
                "#[derive(Reflect)] does not support unions.",
            ));
        }
    };

    let mut field_specs = Vec::with_capacity(fields.len());
    for f in fields {
        let attrs = FieldAttrs::parse(&f.attrs)?;
        field_specs.push((f, attrs));
    }

    codegen::emit_impl(input, &container, &field_specs)
}
