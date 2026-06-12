extern crate proc_macro;

use proc_macro::TokenStream;

use syn::{DeriveInput, parse_macro_input};

use crate::derive::description::derive_description;
use crate::derive::error_code::derive_error_code;

pub(crate) mod derive;
pub(crate) mod utils;

#[proc_macro_derive(Description)]
pub fn description(tokens: TokenStream) -> TokenStream {
    let input = parse_macro_input!(tokens as DeriveInput);

    derive_description(&input).unwrap_or_else(syn::Error::into_compile_error).into()
}

#[proc_macro_derive(ErrorCode, attributes(code))]
pub fn error_code(tokens: TokenStream) -> TokenStream {
    let input = parse_macro_input!(tokens as DeriveInput);

    derive_error_code(&input).unwrap_or_else(syn::Error::into_compile_error).into()
}
