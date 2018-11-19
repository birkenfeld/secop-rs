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
//! Derive a TypeDesc for structs to be used as SECoP Struct types.

use syn::Ident;
use proc_macro2::Span;
use quote::quote;


pub fn derive_typedesc(input: synstructure::Structure) -> proc_macro2::TokenStream {
    match input.ast().data {
        syn::Data::Struct(..) => derive_typedesc_struct(input),
        syn::Data::Enum(..) => derive_typedesc_enum(input),
        _ => panic!("impossible to derive a TypeDesc for unions")
    }
}

pub fn derive_typedesc_struct(input: synstructure::Structure) -> proc_macro2::TokenStream {
    let name = &input.ast().ident;
    let vis = &input.ast().vis;
    let const_name = Ident::new(&format!("_DERIVE_TypeDesc_{}", name), Span::call_site());
    let struct_name = Ident::new(&format!("{}Type", name), Span::call_site());

    let mut descr_members = vec![quote!()];
    // let mut str_arms = Vec::new();
    // let mut int_arms = Vec::new();

    let generated = quote! {
        #vis struct #struct_name;

        #[allow(non_upper_case_globals)]
        const #const_name: () = {
            extern crate serde_json;
            use serde_json::{json, Value};

            impl crate::types::TypeDesc for #struct_name {
                type Repr = #name;
                fn as_json(&self) -> Value {
                    json!(["struct", { #( #descr_members )* }])
                }
                fn from_repr(&self, val: Self::Repr) -> Value {
                    unimplemented!()
                }
                fn to_repr(&self, val: Value) -> std::result::Result<Self::Repr, Value> {
                    unimplemented!()
                }
            }
        };
    };
    // println!("{}", generated);
    generated
}

pub fn derive_typedesc_enum(input: synstructure::Structure) -> proc_macro2::TokenStream {
    let name = &input.ast().ident;
    let vis = &input.ast().vis;
    let const_name = Ident::new(&format!("_DERIVE_TypeDesc_{}", name), Span::call_site());
    let struct_name = Ident::new(&format!("{}Type", name), Span::call_site());

    let mut descr_members = Vec::new();
    let mut str_arms = Vec::new();
    let mut int_arms = Vec::new();

    let mut discr = -1;
    for variant in input.variants() {
        let ident = &variant.ast().ident;
        let ident_str = ident.to_string();
        if let Some((_, dis)) = variant.ast().discriminant {
            if let syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Int(i), .. }) = dis {
                discr = i.value() as i64;
            } else {
                panic!("explicit enum discriminants can only be integer literals")
            }
        } else {
            discr += 1;
        }
        descr_members.push(quote! { #ident_str: #discr, });
        str_arms.push(quote! { #ident_str => Ok(#name::#ident), });
        int_arms.push(quote! { #discr => Ok(#name::#ident), });
    }

    let generated = quote! {
        #vis struct #struct_name;

        #[allow(non_upper_case_globals)]
        const #const_name: () = {
            extern crate serde_json;
            use serde_json::{json, Value};

            impl crate::types::TypeDesc for #struct_name {
                type Repr = #name;
                fn as_json(&self) -> Value {
                    json!(["enum", { #( #descr_members )* }])
                }
                fn from_repr(&self, val: Self::Repr) -> Value {
                    json!(val as i64)
                }
                fn to_repr(&self, val: Value) -> std::result::Result<Self::Repr, Value> {
                    if let Some(s) = val.as_str() {
                        match s {
                            #( #str_arms )*
                            _ => Err(val)
                        }
                    } else if let Some(i) = val.as_i64() {
                        match i {
                            #( #int_arms )*
                            _ => Err(val)
                        }
                    } else {
                        Err(val)
                    }
                }
            }
        };
    };
    // println!("{}", generated);
    generated
}
