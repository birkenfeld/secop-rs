// -----------------------------------------------------------------------------
// Rust SECoP playground
//
// This program is free software; you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free Software
// Foundation; either version 2 of the License, or (at your option) any later
// version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT
// ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE.  See the GNU General Public License for more
// details.
//
// You should have received a copy of the GNU General Public License along with
// this program; if not, write to the Free Software Foundation, Inc.,
// 59 Temple Place, Suite 330, Boston, MA  02111-1307  USA
//
// Module authors:
//   Georg Brandl <g.brandl@fz-juelich.de>
//
// -----------------------------------------------------------------------------
//
//! Derive a SECoP Module implementation for individual modules.

use darling::FromMeta;
use syn::{Expr, Ident};
use syn::spanned::Spanned;
use proc_macro2::Span;
use quote::{quote, quote_spanned};

/// Representation of the #[param(...)] attribute.
#[derive(FromMeta, Debug)]
struct SecopParam {
    name: String,
    doc: String,
    datatype: String,
    readonly: bool,
    #[darling(default)]
    default: Option<String>,
    #[darling(default)]
    unit: String,
    #[darling(default)]
    group: Option<String>,
}

/// Representation of the #[command(...)] attribute.
#[derive(FromMeta, Debug)]
struct SecopCommand {
    name: String,
    doc: String,
    argtype: String,
    restype: String,
}


fn parse_attr<T: FromMeta>(attr: &syn::Attribute) -> Result<T, proc_macro2::TokenStream> {
    attr.parse_meta()
        .map_err(|err| format!("invalid param attribute: {}", err))
        .and_then(|meta| T::from_meta(&meta).map_err(|_| "could not parse this attribute".into()))
        .map_err(|e| quote_spanned! { attr.span() => compile_error!(#e); })
}

pub fn derive_module(input: synstructure::Structure) -> proc_macro2::TokenStream {
    let mut params = Vec::new();
    let mut commands = Vec::new();

    // parse parameter and command attributes on the main struct
    for attr in &input.ast().attrs {
        if attr.path.segments[0].ident == "param" {
            match parse_attr::<SecopParam>(attr) {
                Ok(param) => params.push(param),
                Err(err) => return err
            }
        } else if attr.path.segments[0].ident == "command" {
            match parse_attr::<SecopCommand>(attr) {
                Ok(cmd) => commands.push(cmd),
                Err(err) => return err
            }
        }
    }

    // prepare snippets of code to generate
    let mut statics = vec![];
    let mut par_read_arms = vec![];
    let mut par_write_arms = vec![];
    let mut poll_params = vec![];
    let mut cmd_arms = vec![];
    let mut descriptive = vec![];

    for p in params {
        let SecopParam { name, doc, .. } = p;
        poll_params.push(name.to_string());
        let par_type = Ident::new(&format!("PAR_TYPE_{}", name), Span::call_site());
        let par_expr = syn::parse_str::<Expr>(&p.datatype).expect("unparseable datatype");
        let read_method = Ident::new(&format!("read_{}", name), Span::call_site());
        let write_method = Ident::new(&format!("write_{}", name), Span::call_site());
        statics.push(quote! {
            static ref #par_type : datatype_type!(#par_expr) = #par_expr;
        });
        par_read_arms.push(quote! {
            #name => match self.#read_method() {
                Ok(v)  => #par_type.from_repr(v),
                Err(v) => return Err(Error::new(ErrorKind::BadValue)) // TODO
            }
        });
        par_write_arms.push(if p.readonly {
            quote! {
                #name => return Err(Error::new(ErrorKind::ReadOnly))
            }
        } else {
            quote! {
                #name => match #par_type.to_repr(value.clone()) { // TODO remove clone
                    Ok(v)  => if let Err(e) = self.#write_method(v) { return Err(e) },
                    Err(v) => return Err(Error::new(ErrorKind::BadValue)) // TODO
                }
            }
        });
        descriptive.push(quote! {
            json!([#name, {
                "description": #doc,
                "datatype": #par_expr.as_json(),
            }]),
        });
    }

    for c in commands {
        let SecopCommand { name, doc, .. } = c;
        let arg_type = Ident::new(&format!("CMD_ARG_{}", name), Span::call_site());
        let arg_expr = syn::parse_str::<Expr>(&c.argtype).expect("unparseable datatype");
        let res_type = Ident::new(&format!("CMD_RES_{}", name), Span::call_site());
        let res_expr = syn::parse_str::<Expr>(&c.restype).expect("unparseable datatype");
        let do_method = Ident::new(&format!("do_{}", name), Span::call_site());
        statics.push(quote! {
            static ref #arg_type : datatype_type!(#arg_expr) = #arg_expr;
            static ref #res_type : datatype_type!(#res_expr) = #res_expr;
        });
        cmd_arms.push(quote! {
            #name => match #arg_type.to_repr(arg) {
                Ok(v) => match self. #do_method (v) {
                    Ok(res) => Ok(#res_type.from_repr(res)),
                    Err(e)  => Err(Error::new(ErrorKind::CommandFailed)) // TODO
                },
                Err(v) => Err(Error::new(ErrorKind::BadValue)) // TODO
            }
        });
        descriptive.push(quote! {
            json!([#name, {
                "description": #doc,
                "datatype": Command(#arg_expr, #res_expr).as_json(),
            }]),
        });
    }

    // generate the final code!

    let generated = input.gen_impl(quote! {
        use serde_json::{Value, json};
        use lazy_static::lazy_static;
        use crate::errors::{Error, ErrorKind, Result};
        use crate::types::*;

        lazy_static! {
            #( #statics )*
        }

        gen impl crate::module::ModuleBase for @Self {
            // XXX: this expects an "internals" member...
            fn internals(&self) -> &ModInternals { &self.internals }

            fn describe(&self) -> Value {
                let accessibles = vec![
                    #( #descriptive )*
                ];
                json!([self.name(), {
                    // "description": "TODO",
                    // "visibility": "TODO",
                    // "interface_class": "TODO",
                    // "features": ["TODO"],
                    // "group": "TODO",
                    "accessibles": accessibles
                }])
            }

            fn poll_params(&self) -> &'static [&'static str] {
                &[ #(#poll_params),* ]
            }

            fn change(&mut self, param: &str, value: Value) -> Result<Value> {
                match param {
                    #( #par_write_arms, )*
                    _ => return Err(Error::new(ErrorKind::NoSuchParameter)) // TODO
                }
                Ok(json!([value, {}]))
            }

            fn trigger(&mut self, param: &str) -> Result<Value> {
                let val = match param {
                    #( #par_read_arms, )*
                    _ => return Err(Error::new(ErrorKind::NoSuchParameter)) // TODO
                };
                Ok(json!([val, {}]))
            }

            fn command(&mut self, cmd: &str, arg: Value) -> Result<Value> {
                match cmd {
                    #( #cmd_arms, )*
                    _ => Err(Error::new(ErrorKind::NoSuchCommand)) // TODO
                }
            }
        }
    });
    // println!("{}", generated);
    generated
}