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

use syn::{Expr, Ident, spanned::Spanned};
use proc_macro2::Span;
use quote::{quote, quote_spanned};
use darling::FromMeta;

fn default_polling() -> i64 { 1 }

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
    group: String,
    #[darling(default = "default_polling")]
    polling: i64,
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

    let vis = &input.ast().vis;
    let poll_struct_name = Ident::new(&format!("{}PollParams", input.ast().ident),
                                      Span::call_site());

    // TODO: check lowercase-uniqueness of params

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
    let mut cmd_arms = vec![];
    let mut descriptive = vec![];
    let mut poll_struct = vec![];
    let mut poll_busy_params = vec![];
    let mut poll_other_params = vec![];

    for p in params {
        // TODO: process default
        let SecopParam { name, doc, readonly, datatype, unit, group, polling, .. } = p;
        let type_static = Ident::new(&format!("PAR_TYPE_{}", name), Span::call_site());
        let type_expr = syn::parse_str::<Expr>(&datatype).expect("unparseable datatype");
        let read_method = Ident::new(&format!("read_{}", name), Span::call_site());
        let write_method = Ident::new(&format!("write_{}", name), Span::call_site());
        statics.push(quote! {
            static ref #type_static : datatype_type!(#type_expr) = #type_expr;
        });
        par_read_arms.push(quote! {
            #name => #type_static.to_json(self.#read_method()?)?
        });
        par_write_arms.push(if p.readonly {
            quote! { #name => return Err(Error::new(ErrorKind::ReadOnly, "")) }
        } else {
            quote! { #name => self.#write_method(#type_static.from_json(&value)?)? }
        });
        if polling != 0 {
            let name_id = Ident::new(&name, Span::call_site());
            poll_struct.push(quote! {
                #name_id : <datatype_type!(#type_expr) as TypeDesc>::Repr,
            });
            let polling_abs = polling.abs() as usize;
            // TODO error handling
            let poll_it = quote! {
                if n % #polling_abs == 0 {
                    if let Ok(value) = self.#read_method() {
                        if value != pp.#name_id {
                            pp.#name_id = value.clone();
                            if let Ok(val_json) = #type_static.to_json(value) {
                                self.rep_sender().send((Option::None,
                                                        Msg::Update { module: self.name().into(),
                                                                      param: #name.into(),
                                                                      value: val_json })).unwrap();
                            }
                        }
                    }
                }
            };
            if polling > 0 {
                poll_busy_params.push(poll_it);
            } else {
                poll_other_params.push(poll_it);
            }
        }
        let unit_entry = if !unit.is_empty() { quote! { "unit": #unit, } } else { quote! {} };
        let group_entry = if !group.is_empty() { quote! { "group": #group, } } else { quote! {} };
        descriptive.push(quote! {
            json!([#name, {
                "description": #doc,
                "datatype": #type_static.type_json(),
                "readonly": #readonly,
                #unit_entry
                #group_entry
            }]),
        });
    }

    for c in commands {
        let SecopCommand { name, doc, .. } = c;
        let argtype_static = Ident::new(&format!("CMD_ARG_{}", name), Span::call_site());
        let argtype_expr = syn::parse_str::<Expr>(&c.argtype).expect("unparseable datatype");
        let restype_static = Ident::new(&format!("CMD_RES_{}", name), Span::call_site());
        let restype_expr = syn::parse_str::<Expr>(&c.restype).expect("unparseable datatype");
        let do_method = Ident::new(&format!("do_{}", name), Span::call_site());
        statics.push(quote! {
            static ref #argtype_static : datatype_type!(#argtype_expr) = #argtype_expr;
            static ref #restype_static : datatype_type!(#restype_expr) = #restype_expr;
        });
        cmd_arms.push(quote! {
            #name => #restype_static.to_json(self.#do_method(#argtype_static.from_json(&arg)?)?)
        });
        descriptive.push(quote! {
            json!([#name, {
                "description": #doc,
                "datatype": ["command", #argtype_static.type_json(), #restype_static.type_json()],
            }]),
        });
    }

    let poll_busy_params = &poll_busy_params;

    // generate the final code!
    let generated = input.gen_impl(quote! {
        use serde_json::{Value, json};
        use lazy_static::lazy_static;
        use crate::errors::{Error, ErrorKind, Result};
        use crate::proto::Msg;
        use crate::util::localtime;

        lazy_static! {
            #( #statics )*
        }

        #[derive(Default)]
        #vis struct #poll_struct_name {
            #( #poll_struct )*
        }

        gen impl crate::module::ModuleBase for @Self {
            type PollParams = #poll_struct_name;

            // XXX: this expects an "internals" member...
            fn internals(&self) -> &ModInternals { &self.internals }

            fn describe(&self) -> Value {
                let accessibles = vec![
                    #( #descriptive )*
                ];
                json!([self.name(), {
                    "description": self.config().description,
                    // "visibility": "TODO",
                    // "interface_class": "TODO",
                    // "features": ["TODO"],
                    "group": self.config().group,
                    "accessibles": accessibles
                }])
            }

            fn change(&mut self, param: &str, value: Value) -> Result<Value> {
                match param {
                    #( #par_write_arms, )*
                    _ => return Err(Error::new(ErrorKind::NoSuchParameter, ""))
                }
                // TODO: potentially emit change message here
                Ok(json!([value, {"t": localtime()}]))
            }

            fn trigger(&mut self, param: &str) -> Result<Value> {
                let value = match param {
                    #( #par_read_arms, )*
                    _ => return Err(Error::new(ErrorKind::NoSuchParameter, ""))
                };
                Ok(json!([value, {"t": localtime()}]))
            }

            fn command(&mut self, cmd: &str, arg: Value) -> Result<Value> {
                match cmd {
                    #( #cmd_arms, )*
                    _ => Err(Error::new(ErrorKind::NoSuchCommand, ""))
                }
            }

            fn poll_normal(&mut self, n: usize, pp: &mut Self::PollParams) {
                if pp.status.0 != StatusConst::Busy {
                    #( #poll_busy_params )*
                }
                #( #poll_other_params )*
            }

            fn poll_busy(&mut self, n: usize, pp: &mut Self::PollParams) {
                if pp.status.0 == StatusConst::Busy {
                    #( #poll_busy_params )*
                }
            }
        }
    });
    // println!("{}", generated);
    generated
}
