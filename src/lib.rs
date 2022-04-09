#![doc = include_str!("../README.md")]
#![warn(rust_2018_idioms)]

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    spanned::Spanned,
    token::Comma,
    Error, FnArg, ItemFn, Result, ReturnType, Signature, TraitBound,
};

struct Item(ItemFn);
struct Attrs(Punctuated<TraitBound, Comma>);

impl Parse for Item {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let item_fn = ItemFn::parse(input)?;

        if item_fn.sig.asyncness.is_none() {
            return Err(Error::new(
                item_fn.sig.fn_token.span(),
                "`async-into-future` can only be used on async functions",
            ));
        }
        if item_fn.sig.constness.is_some() {
            return Err(Error::new(
                item_fn.sig.constness.span(),
                "`async-into-future` cannot be used on const functions",
            ));
        }
        if item_fn.sig.variadic.is_some() {
            return Err(Error::new(
                item_fn.sig.variadic.span(),
                "`async-into-future` cannot be used on variadic functions",
            ));
        }
        if let Some(FnArg::Receiver(s)) = item_fn.sig.inputs.first() {
            return Err(Error::new(
                s.span(),
                "`async-into-future` cannot be used inside impl blocks",
            ));
        }

        Ok(Item(item_fn))
    }
}
impl Parse for Attrs {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        input.parse_terminated(TraitBound::parse).map(Attrs)
    }
}

#[proc_macro_attribute]
pub fn async_into_future(attr: TokenStream, item: TokenStream) -> TokenStream {
    let Item(ItemFn {
        attrs,
        vis,
        sig:
            Signature {
                unsafety,
                abi,
                ident,
                generics,
                inputs,
                output,
                ..
            },
        block,
    }) = parse_macro_input!(item as Item);
    let Attrs(traits) = parse_macro_input!(attr as Attrs);

    let typed_inputs = {
        let i = inputs.iter();
        quote!(#(#i),*)
    };
    let untyped_inputs = {
        let i = inputs.iter().filter_map(|i| match i {
            FnArg::Typed(t) => Some(&t.pat),
            _ => None,
        });
        quote!(#(#i),*)
    };

    let output = match output {
        ReturnType::Default => quote!(()),
        ReturnType::Type(_, ty) => quote!(#ty),
    };

    let ty = format_ident!("__AsyncIntoFuture__{}", ident);

    let traits = traits.into_iter();
    let lifetimes = generics.lifetimes();
    let future = quote! {
        ::std::pin::Pin<::std::boxed::Box<
            dyn ::std::future::Future<Output = #output>
                #(+ #traits)*
                #(+ #lifetimes)*
        >>
    };

    quote! {
        #[allow(non_camel_case_types)]
        struct #ty #generics {
            #typed_inputs
        }

        impl #generics ::std::future::IntoFuture for #ty #generics {
            type IntoFuture = #future;
            type Output = #output;

            fn into_future(self) -> Self::IntoFuture {
                let Self { #untyped_inputs } = self;
                ::std::boxed::Box::pin(async move #block)
            }
        }

        #(#attrs) *
        #vis #unsafety #abi fn #ident #generics (#typed_inputs) -> #ty {
            #ty { #untyped_inputs }
        }
    }
    .into()
}
