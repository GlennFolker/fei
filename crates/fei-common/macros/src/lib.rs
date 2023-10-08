extern crate proc_macro;

use proc_macro2::TokenStream;
use quote::{
    format_ident, quote,
};
use syn::{
    parse::{
        Parse, ParseStream,
    },
    Ident, LitInt,
    parse_macro_input,
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
    let input = parse_macro_input!(input as ImplTuples);

    match (move || -> syn::Result<TokenStream> {
        let implementor = input.implementor;
        let start = if input.second.is_some() { input.first.base10_parse::<usize>()? } else { 0 };
        let amount = if let Some(second) = input.second { second.base10_parse::<usize>()? } else { input.first.base10_parse::<usize>()? };

        let mut output = Vec::<TokenStream>::new();
        (start..=amount).for_each(|i| {
            let params = (start..start + i).fold(Vec::<TokenStream>::new(), |mut to, accum| {
                let idx = syn::parse_str::<LitInt>(&(accum - start).to_string()).unwrap();
                let type_idx = format_ident!("T{idx}");

                to.push(quote! {
                    #type_idx #idx
                });
                to
            });

            output.push(quote! {
                #implementor!(#(#params),*)
            });
        });

        syn::parse2(quote! {
            #(#output;)*
        })
    })() {
        Ok(stream) => stream.into(),
        Err(e) => e.to_compile_error().into(),
    }
}
