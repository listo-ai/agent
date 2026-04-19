//! Expansion for `#[derive(NodeKind)]`.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse2, DeriveInput, Result};

use crate::attrs::NodeAttrs;
use crate::manifest::{self, LoadedManifest};

pub(crate) fn expand(input: TokenStream) -> Result<TokenStream> {
    let ast: DeriveInput = parse2(input)?;
    let ty = &ast.ident;
    let attrs = NodeAttrs::parse(&ast.attrs)?;

    // `behavior` is parsed for its forcing-function effect (compile
    // error if absent) but doesn't drive code generation in 3a-1;
    // NodeBehavior dispatch lands in 3a-2 and will consume it here.
    let _ = &attrs.behavior;

    let LoadedManifest {
        absolute_path,
        parsed,
    } = manifest::load(&attrs.manifest_path, attrs.span)?;

    if parsed.id.as_str() != attrs.kind {
        return Err(syn::Error::new(
            attrs.span,
            format!(
                "kind id mismatch: attribute says `{}` but manifest `{}` declares `{}`",
                attrs.kind, absolute_path, parsed.id,
            ),
        ));
    }

    let kind_literal = attrs.kind;

    Ok(quote! {
        impl ::extensions_sdk::NodeKind for #ty {
            fn kind_id() -> ::extensions_sdk::__private::spi::KindId {
                ::extensions_sdk::__private::spi::KindId::new(#kind_literal)
            }

            fn manifest() -> ::extensions_sdk::__private::spi::KindManifest {
                // Manifest YAML is the single source of truth. It was
                // parsed + validated at compile time; a runtime failure
                // here means the compiled-in bytes were tampered with.
                const YAML: &str = include_str!(#absolute_path);
                ::extensions_sdk::__private::serde_yml::from_str(YAML)
                    .expect("manifest YAML validated at compile time")
            }
        }
    })
}
