use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{self, Data, DeriveInput};

use crate::utils::doc_comments::extract_doc_comment;

pub(crate) fn derive_description(input: &DeriveInput) -> syn::Result<TokenStream> {
    let name = input.ident.clone();
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    match input.data {
        Data::Enum(ref e) => {
            let variants = e
                .variants
                .iter()
                .map(|variant| {
                    let variant_name = variant.ident.clone();
                    let comments = extract_doc_comment(&variant.attrs);

                    if !comments.is_empty() {
                        let description = comments.join(" ");

                        quote! {
                            #name::#variant_name => Some(#description.to_string()),
                        }
                    } else {
                        quote! {
                            #name::#variant_name => None,
                        }
                    }
                })
                .collect::<Vec<_>>();

            let modified = quote! {
                impl #impl_generics ::lit_core::types::description::Description for #name #ty_generics #where_clause {
                    fn description(&self) -> Option<String> {
                        match self {
                            #(#variants)*
                        }
                    }
                }
            };

            Ok(modified)
        }
        _ => {
            Err(syn::Error::new(Span::call_site(), "`#[derive(Description)]` only supports enums"))
        }
    }
}
