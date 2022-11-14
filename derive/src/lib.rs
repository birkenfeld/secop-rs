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
//! # Derive support for secop modules
//!
//! There are two auto-derive traits implemented here:
//!
//! * `ModuleBase` is a complete implementation of the guts of a module.  It
//!   provides an easy DSL to add parameters and commands, and translates that
//!   into the respective case handling in the methods that implement the
//!   basic SECoP actions like `change` and `do`.
//!
//!   It also provides automatic translation and verification between JSON
//!   payloads and Rust data for parameter and argument types.
//!
//! * `TypeInfo` can be derived for enums and structs, and provides a type-
//!   safe way to declare parameters and commands with enum and struct
//!   datatypes.

mod module;
mod typeinfo;

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{Error, Expr};
use synstructure::decl_derive;

decl_derive!([ModuleBase, attributes(param, command)] => crate::module::derive_module);
decl_derive!([TypeInfo, attributes(datainfo)] => crate::typeinfo::derive_typeinfo);


// Common helpers

/// Translate a short "datainfo" attribute value into (datainfo type, datainfo value).
fn translate_datainfo(span: Span, input: Expr) -> Result<(TokenStream, TokenStream), Error> {
    match input {
        // Simple names remain
        Expr::Path(p) => {
            Ok((quote!(#p), quote!(#p)))
        }
        // A(opt=1) -> struct syntax
        Expr::Call(c) => {
            let mut converted = vec![];
            for arg in c.args {
                if let Expr::Assign(a) = arg {
                    let key = a.left;
                    let val = a.right;
                    converted.push(quote!( .#key(#val) ));
                } else {
                    return Err(Error::new(span, "Type(opt=val, ...) expected"));
                }
            }
            let typ = c.func;
            Ok((quote!(#typ), quote!(#typ::new() #(#converted)*)))
        }
        // (A, B, C) -> tuple of subtypes
        Expr::Tuple(t) => {
            let mut members = vec![];
            let mut members_t = vec![];
            let tuple_n = format_ident!("Tuple{}", t.elems.len());
            for elem in t.elems {
                let (mem_t, mem) = translate_datainfo(span, elem)?;
                members_t.push(mem_t);
                members.push(mem);
            }
            Ok((quote!(#tuple_n< #(#members_t),* >),
                quote!(#tuple_n( #(#members),* ))))
        }
        // [A; 5] or [A; 1..5] -> array
        Expr::Repeat(a) => {
            let (min, max) = match *a.len {
                Expr::Lit(l) => (quote!(0), quote!(#l)),
                Expr::Range(r) if r.from.is_some() && r.to.is_some() &&
                    matches!(r.limits, syn::RangeLimits::Closed(_)) =>
                {
                    let from = r.from.unwrap();
                    let to = r.to.unwrap();
                    (quote!(#from), quote!(#to))
                }
                _ => return Err(Error::new(span, "[Type; N] or [Type; N..=M] expected")),
            };
            let (sub_t, sub) = translate_datainfo(span, *a.expr)?;
            Ok((quote!(ArrayOf<#sub_t>),
                quote!(ArrayOf { minlen: #min, maxlen: #max, members: #sub })))
        }
        _ => Err(Error::new(span, "invalid datainfo: expected Type, Type(options), \
                                   (Type, ...), or [Type; N{..=M}]")),
    }
}

pub(crate) fn parse_datainfo(span: Span, input: &str) -> Result<(TokenStream, TokenStream), Error> {
    match syn::parse_str::<Expr>(input) {
        Ok(d) => translate_datainfo(span, d),
        Err(e) => Err(Error::new(span, format!("invalid datainfo: {}", e))),
    }
}
