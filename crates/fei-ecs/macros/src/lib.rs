extern crate proc_macro;

use fei_macros::prelude::*;

use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    self,
    DeriveInput,
    Error, Ident, LitStr,
};
use fei_macros::proc_macro2::Span;

#[proc_macro_derive(Component, attributes(component))]
pub fn impl_tuples(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    match (move || -> syn::Result<TokenStream> {
        let mut input = syn::parse::<DeriveInput>(input)?;
        let fei_ecs = fei_macros::module("fei-ecs")?.ok_or_else(|| Error::new_spanned(&input, "`fei-ecs` is unavailable"))?;

        let mut storage = "Table".to_string();
        for meta in input.attrs.iter().filter(|a| a.path().is_ident("component")) {
            meta.parse_nested_meta(|meta| if meta.path.is_ident("storage") {
                storage = match meta.value()?.parse::<LitStr>()?.value() {
                    s if s == "Table" || s == "SparseSet" => s,
                    s => return Err(meta.error(format!("Invalid storage type `{s}`, expected `Table` or `SparseSet`."))),
                };
                Ok(())
            } else {
                Err(meta.error("Unsupported component attribute"))
            })?;
        }

        let storage = {
            let storage = Ident::new(&storage, Span::call_site());
            quote! { #fei_ecs::component::ComponentStorage::#storage }
        };

        input.generics
            .make_where_clause()
            .predicates
            .push(syn::parse2(quote! { Self: Send + Sync + 'static })?);

        let target = &input.ident;
        let (impl_generics, type_generics, where_clause) = &input.generics.split_for_impl();

        Ok(quote! {
            impl #impl_generics #fei_ecs::component::Component for #target #type_generics #where_clause {
                const STORAGE: #fei_ecs::component::ComponentStorage = #storage;
            }
        })
    })() {
        Ok(stream) => stream.into(),
        Err(e) => e.to_compile_error().into(),
    }
}
