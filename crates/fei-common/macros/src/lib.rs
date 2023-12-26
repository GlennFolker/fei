extern crate proc_macro;

use fei_macros::prelude::*;

use proc_macro2::TokenStream;
use quote::{
    format_ident, quote,
};
use syn::{
    parse::{
        Parse, ParseStream,
    },
    Ident, Index, LitInt,
    Token,
};

struct ImplTuples {
    implementor: Ident,
    _exclamation: Token![!],
    first: LitInt,
    second: Option<LitInt>,
}

impl Parse for ImplTuples {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(Self {
            implementor: input.parse()?,
            _exclamation: input.parse()?,
            first: input.parse()?,
            second: input.parse()?,
        })
    }
}

/// Invokes a macro repeatedly with `T0 0`, `T0 0, T1 1`, `T0 0, T1 1, T2 2,` and so on. This is
/// typically used to implement traits for tuple of trait derivatives.
///
/// The example below implements `MyTrait` for any tuple with `[0, 8]` elements that derive `MyTrait`.
/// ```
/// use fei_common_macros::impl_tuples;
///
/// trait MyTrait {
///     type Assoc;
///
///     fn my_instance_function(&self);
///
///     fn my_static_function();
/// }
///
/// macro_rules! impl_my_trait {
///     ($($tuple_type:ident $tuple_index:tt),*) => {
///         impl<$($tuple_type: MyTrait,)*> MyTrait for ($($tuple_type,)*) {
///             type Assoc = ($($tuple_type::Assoc,)*);
///
///             fn my_instance_function(&self) {
///                 $(self.$tuple_index.my_instance_function();)*
///             }
///
///             fn my_static_function() {
///                 $($tuple_type::my_static_function();)*
///             }
///         }
///     };
/// } impl_tuples!(impl_my_trait! 8);
/// ```
#[proc_macro]
pub fn impl_tuples(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    match (move || -> syn::Result<TokenStream> {
        let input = syn::parse::<ImplTuples>(input)?;

        let implementor = input.implementor;
        let a = input.first.base10_parse()?;
        let b = input.second.map(|b| b.base10_parse()).transpose()?;

        let start = if b.is_some() { a } else { 0 };
        let amount = if let Some(b) = b { b } else { a };

        let calls = (start..=amount).fold(Vec::<TokenStream>::with_capacity(amount - start + 1), |mut calls, i| {
            calls.push({
                let params = (start..start + i).fold(Vec::<TokenStream>::with_capacity(i), |mut params, accum| {
                    let idx = Index::from(accum - start);
                    let type_idx = format_ident!("T{}", accum - start);

                    params.push(quote! { #type_idx #idx });
                    params
                });

                quote! { #implementor!(#(#params),*) }
            });
            calls
        });

        Ok(quote! { #(#calls;)* })
    })() {
        Ok(stream) => stream.into(),
        Err(e) => e.to_compile_error().into(),
    }
}
