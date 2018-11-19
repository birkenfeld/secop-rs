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
//! Derive a SeCOP Module implementation for individual modules.

use darling::FromMeta;
use syn::Ident;
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
    let mut partype_names = vec![];
    let mut partype_types = vec![];
    let mut par_names = vec![];
    let mut par_read_arms = vec![];
    let mut par_write_arms = vec![];

    let mut cmdarg_names = vec![];
    let mut cmdarg_types = vec![];
    let mut cmdres_names = vec![];
    let mut cmdres_types = vec![];
    let mut cmd_names = vec![];
    let mut cmd_arms = vec![];

    for p in params {
        par_names.push(p.name.to_string());
        let par_type = Ident::new(&format!("PAR_TYPE_{}", p.name), Span::call_site());
        let read_method = Ident::new(&format!("read_{}", p.name), Span::call_site());
        let write_method = Ident::new(&format!("write_{}", p.name), Span::call_site());
        par_read_arms.push(quote! {
            match self.#read_method() {
                Ok(v)  => #par_type.from_repr(v),
                Err(v) => return Err(Error::new(ErrorKind::BadValue)) // TODO
            }
        });
        par_write_arms.push(if p.readonly {
            quote! {
                return Err(Error::new(ErrorKind::ReadOnly))
            }
        } else {
            quote! {
                match #par_type.to_repr(value.clone()) { // TODO remove clone
                    Ok(v)  => if let Err(e) = self.#write_method(v) { return Err(e) },
                    Err(v) => return Err(Error::new(ErrorKind::BadValue)) // TODO
                }
            }
        });
        partype_names.push(par_type);
        partype_types.push(syn::parse_str::<syn::Expr>(&p.datatype).expect("unparseable datatype"));
    }

    for c in commands {
        cmd_names.push(c.name.to_string());
        let arg_type = Ident::new(&format!("CMD_ARG_{}", c.name), Span::call_site());
        let res_type = Ident::new(&format!("CMD_RES_{}", c.name), Span::call_site());
        let do_method = Ident::new(&format!("do_{}", c.name), Span::call_site());
        cmd_arms.push(quote! {
            match #arg_type.to_repr(arg) {
                Ok(v) => match self. #do_method (v) {
                    Ok(res) => Ok(#res_type.from_repr(res)),
                    Err(e)  => Err(Error::new(ErrorKind::CommandFailed)) // TODO
                },
                Err(v) => Err(Error::new(ErrorKind::BadValue)) // TODO
            }
        });
        cmdarg_names.push(arg_type);
        cmdres_names.push(res_type);
        cmdarg_types.push(syn::parse_str::<syn::Expr>(&c.argtype).expect("unparseable datatype"));
        cmdres_types.push(syn::parse_str::<syn::Expr>(&c.restype).expect("unparseable datatype"));
    }

    let partype_exprs = partype_types.clone();
    let cmdarg_exprs = cmdarg_types.clone();
    let cmdres_exprs = cmdres_types.clone();
    let par_names_2 = par_names.clone();
    let par_names_3 = par_names.clone();

    // generate the final code!

    let generated = input.gen_impl(quote! {
        use serde_json::{Value, json};
        use lazy_static::lazy_static;
        use crate::errors::{Error, ErrorKind, Result};
        use crate::types::*;

        lazy_static! {
            #( static ref #partype_names : datatype_type!(#partype_types) = #partype_exprs; )*
            #( static ref #cmdarg_names : datatype_type!(#cmdarg_types) = #cmdarg_exprs; )*
            #( static ref #cmdres_names : datatype_type!(#cmdres_types) = #cmdres_exprs; )*
        }

        gen impl crate::module::ModuleBase for @Self {
            // XXX: this expects an "internals" member...
            fn internals(&self) -> &ModInternals { &self.internals }

            fn describe(&self) -> Value {
                Value::Null // TODO
            }

            fn poll_params(&self) -> &'static [&'static str] {
                &[#(#par_names),*]
            }

            fn change(&mut self, param: &str, value: Value) -> Result<Value> {
                match param {
                    #( #par_names_2 => { #par_write_arms }, )*
                    _ => return Err(Error::new(ErrorKind::NoSuchParameter)) // TODO
                }
                Ok(json!([value, {}]))
            }

            fn trigger(&mut self, param: &str) -> Result<Value> {
                let val = match param {
                    #( #par_names_3 => { #par_read_arms }, )*
                    _ => return Err(Error::new(ErrorKind::NoSuchParameter)) // TODO
                };
                Ok(json!([val, {}]))
            }

            fn command(&mut self, cmd: &str, arg: Value) -> Result<Value> {
                match cmd {
                    #( #cmd_names => { #cmd_arms }, )*
                    _ => Err(Error::new(ErrorKind::NoSuchCommand)) // TODO
                }
            }
        }
    });
    // println!("{}", generated);
    generated
}
