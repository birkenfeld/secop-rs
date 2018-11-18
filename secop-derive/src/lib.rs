//! Derive support for secop modules.

#![recursion_limit="128"]

extern crate proc_macro2;
extern crate quote;
extern crate syn;
#[macro_use]
extern crate synstructure;
extern crate darling;

use darling::FromMeta;
use syn::Ident;
use syn::spanned::Spanned;
use proc_macro2::Span;


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


#[derive(FromMeta, Debug)]
struct SecopCommand {
    name: String,
    doc: String,
    argtype: String,
    restype: String,
}


fn derive_module(input: synstructure::Structure) -> proc_macro2::TokenStream {
    let mut params = Vec::new();
    let mut commands = Vec::new();

    // parse parameter and command attributes on the main struct
    for attr in &input.ast().attrs {
        if attr.path.segments.len() == 1 && attr.path.segments[0].ident == "param" {
            if let Ok(meta) = attr.parse_meta() {
                match SecopParam::from_meta(&meta) {
                    Ok(pinfo) => params.push(pinfo),
                    Err(err) => {
                        let errmsg = format!("invalid param attribute: {}", err);
                        return quote_spanned!(attr.span() => compile_error!{#errmsg});
                    }
                }
            } else {
                return quote_spanned!(attr.span() =>
                                      compile_error!{"could not parse this attribute"});
            }
        }
        if attr.path.segments.len() == 1 && attr.path.segments[0].ident == "command" {
            if let Ok(meta) = attr.parse_meta() {
                match SecopCommand::from_meta(&meta) {
                    Ok(cinfo) => commands.push(cinfo),
                    Err(err) => {
                        let errmsg = format!("invalid command attribute: {}", err);
                        return quote_spanned!(attr.span() => compile_error!{#errmsg});
                    }
                }
            } else {
                return quote_spanned!(attr.span() =>
                                      compile_error!{"could not parse this attribute"});
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
            match self. #read_method () {
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
                    Ok(v)  => { self. #write_method (v).unwrap(); } // TODO can return error
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
        use serde_json::Value;
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

decl_derive!([ModuleBase, attributes(param, command)] => derive_module);
