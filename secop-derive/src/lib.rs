//! Derive support for secop modules.

#![recursion_limit="128"]

#[macro_use]
extern crate quote;
#[macro_use]
extern crate syn;

extern crate proc_macro;
extern crate proc_macro2;

use self::proc_macro::TokenStream;

#[proc_macro_derive(Module, attributes(secop))]
pub fn derive_process_image(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    let ident = input.ident;

    let generated = quote! {
        #[automatically_derived]
        impl Module for #ident {
            fn create(config: &crate::module::Config) -> Self { unimplemented!() }
            fn get_api_description(&self) -> ::serde_json::Value { unimplemented!() }
            fn change(&mut self, param: &str, value: ::serde_json::Value) ->
                Result<(), crate::errors::Error> { unimplemented!() }
            fn command(&mut self, cmd: &str, args: ::serde_json::Value) ->
                Result<::serde_json::Value, crate::errors::Error> { unimplemented!() }
            fn trigger(&mut self, param: &str) ->
                Result<::serde_json::Value, crate::errors::Error> { unimplemented!() }
        }
    };

    // println!("{}", generated);
    generated.into()
}
