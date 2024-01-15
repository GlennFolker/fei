extern crate proc_macro;

use fei_macros::prelude::*;

use proc_macro2::{
    Span, TokenStream,
};
use quote::{
    quote,
    ToTokens,
};
use syn::{
    self,
    spanned::Spanned,
    DeriveInput,
    Data, Error, Fields, Ident, Index, LitStr,
};

#[proc_macro_derive(Component, attributes(component))]
pub fn derive_component(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    match (move || -> syn::Result<TokenStream> {
        let mut input = syn::parse::<DeriveInput>(input)?;
        let fei_ecs = fei_macros::module("fei-ecs")?.ok_or_else(|| Error::new_spanned(&input, "`fei-ecs` is unavailable"))?;

        let mut storage = "Table".to_string();
        for meta in input.attrs.iter().filter(|&attr| attr.path().is_ident("component")) {
            meta.parse_nested_meta(|meta| if meta.path.is_ident("storage") {
                storage = match meta.value()?.parse::<LitStr>()?.value() {
                    s if s == "Table" || s == "SparseSet" => s,
                    s => return Err(meta.error(format!("Invalid storage type `{s}`, expected `Table` or `SparseSet`."))),
                };
                Ok(())
            } else {
                Err(meta.error("Unsupported `Component` attribute"))
            })?;
        }

        let storage = {
            let storage = Ident::new(&storage, Span::call_site());
            quote! { #fei_ecs::component::ComponentStorage::#storage }
        };

        input.generics
            .make_where_clause()
            .predicates
            .push(syn::parse2(quote! { Self: 'static + Send + Sync + Sized })?);

        let target = &input.ident;
        let (impl_generics, type_generics, where_clause) = &input.generics.split_for_impl();

        Ok(quote! {
            impl #impl_generics #fei_ecs::component::Component for #target #type_generics #where_clause {
                const STORAGE: #fei_ecs::component::ComponentStorage = #storage;
            }
        })
    })() {
        Ok(stream) => stream,
        Err(e) => e.to_compile_error(),
    }.into()
}

#[proc_macro_derive(ComponentSet)]
pub fn derive_component_set(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    match (move || -> syn::Result<TokenStream> {
        let mut input = syn::parse::<DeriveInput>(input)?;
        let fei_ecs = fei_macros::module("fei-ecs")?.ok_or_else(|| Error::new_spanned(&input, "`fei-ecs` is unavailable."))?;

        let Data::Struct(data) = &input.data else {
            return Err(Error::new_spanned(&input, "Only `struct`s are allowed for deriving `ComponentSet`."))
        };

        if let Fields::Unit = data.fields {
            return Err(Error::new_spanned(&input, "Unit `struct`s may not derive `ComponentSet`."))
        }

        input.generics
            .make_where_clause()
            .predicates
            .push(syn::parse2(quote! { Self: 'static + Send + Sync + Sized })?);

        let len = data.fields.len();
        if len == 0 {
            return Err(Error::new_spanned(&input, "`ComponentSet` structs must have at least 1 field."));
        }

        let (fields, types, asserts) = data.fields
            .iter().enumerate()
            .try_fold(
                (Vec::with_capacity(len), Vec::with_capacity(len), Vec::with_capacity(len)),
                |(mut fields, mut types, mut asserts), (index, field)| {
                    let id = field.ident.as_ref()
                        .map(ToTokens::to_token_stream)
                        .unwrap_or_else(|| Index { index: index as u32, span: field.span(), }.into_token_stream());
                    let ty = field.ty.clone();

                    input.generics
                        .make_where_clause()
                        .predicates
                        .push(syn::parse2(quote! { #ty: #fei_ecs::component::ComponentSet })?);

                    fields.push(id.clone());
                    types.push(ty.clone());
                    asserts.push(format!("field `{id}` of type `{}` isn't aligned", ty.to_token_stream()));

                    Ok::<_, Error>((fields, types, asserts))
                },
            )?;

        let target = &input.ident;
        let (impl_generics, type_generics, where_clause) = &input.generics.split_for_impl();

        Ok(quote! {
            unsafe impl #impl_generics #fei_ecs::component::ComponentSet for #target #type_generics #where_clause {
                #[inline]
                fn metadata(base_offset: usize, callback: &mut impl FnMut(usize, std::any::TypeId, #fei_ecs::component::ComponentInfo)) {
                    let uninit = std::mem::MaybeUninit::<Self>::uninit();
                    let base = uninit.as_ptr();

                    #(
                        <#types>::metadata(base_offset + unsafe {
                            let addr = std::ptr::addr_of!((*base).#fields);
                            assert_eq!(
                                addr.align_offset(std::mem::align_of::<#types>()), 0,
                                #asserts,
                            );

                            addr as usize - base as usize
                        }, callback);
                    )*
                }
            }
        })
    })() {
        Ok(stream) => stream,
        Err(e) => e.to_compile_error(),
    }.into()
}

#[proc_macro_derive(Resource)]
pub fn derive_resource(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    match derive_resource_generic(input, false) {
        Ok(stream) => stream,
        Err(e) => e.to_compile_error(),
    }.into()
}

#[proc_macro_derive(ResourceLocal)]
pub fn derive_resource_local(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    match derive_resource_generic(input, true) {
        Ok(stream) => stream,
        Err(e) => e.to_compile_error(),
    }.into()
}

#[inline]
fn derive_resource_generic(input: proc_macro::TokenStream, local: bool) -> syn::Result<TokenStream> {
    let mut input = syn::parse::<DeriveInput>(input)?;
    let fei_ecs = fei_macros::module("fei-ecs")?.ok_or_else(|| Error::new_spanned(&input, "`fei-ecs` is unavailable."))?;
    let which = Ident::new(if local { "ResourceLocal" } else { "Resource" }, Span::call_site());

    input.generics
        .make_where_clause()
        .predicates
        .push(syn::parse2(if local {
            quote! { Self: 'static + Sized }
        } else {
            quote! { Self: 'static + Send + Sync + Sized }
        })?);

    let target = &input.ident;
    let (impl_generics, type_generics, where_clause) = &input.generics.split_for_impl();

    Ok(quote! {
        impl #impl_generics #fei_ecs::resource::#which for #target #type_generics #where_clause {}
    })
}
