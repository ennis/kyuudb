#![recursion_limit = "256"]

mod store;

//#![feature(proc_macro_diagnostic)]
extern crate proc_macro;

use proc_macro2::{Span, TokenStream};
use quote::{quote, ToTokens, TokenStreamExt};
use syn::{spanned::Spanned, MetaList};

//--------------------------------------------------------------------------------------------------

struct CrateName;

/// Object that expands to the crate name when quoted.
const CRATE: CrateName = CrateName;

impl ToTokens for CrateName {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        tokens.append(syn::Ident::new("kyuudb", Span::call_site()))
    }
}

//--------------------------------------------------------------------------------------------------

fn try_generate(
    input: proc_macro::TokenStream,
    f: fn(proc_macro::TokenStream) -> syn::Result<TokenStream>,
) -> proc_macro::TokenStream {
    match f(input) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.into_compile_error().into(),
    }
}

/// Implements a data store with the specified schema.
///
/// TODO docs
#[proc_macro]
pub fn store(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    try_generate(input, store::generate_store)
}
