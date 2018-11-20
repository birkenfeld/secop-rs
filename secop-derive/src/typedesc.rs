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

use syn::{Ident, Expr};
use proc_macro2::Span;
use quote::quote;
use darling::FromMeta;


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

    let mut statics = Vec::new();
    let mut members = Vec::new();
    let mut member_names = Vec::new();
    let mut member_to_json = Vec::new();
    let mut member_contains = Vec::new();
    let mut member_from_json = Vec::new();
    let mut descr_members = Vec::new();

    for binding in input.variants()[0].bindings() {
        let ident = &binding.ast().ident.as_ref().unwrap_or_else(
            || panic!("TypeDesc cannot be derived for tuple structs"));
        let ident_str = ident.to_string();
        let dtype_static = Ident::new(&format!("STRUCT_FIELD_{}", ident_str), Span::call_site());
        let mut dtype = None;
        for attr in &binding.ast().attrs {
            if attr.path.segments[0].ident == "datatype" {
                dtype = attr.interpret_meta();
            }
        }
        let dtype = dtype.unwrap_or_else(
            || panic!("member {} has no valid datatype attribute", ident_str));
        let dtype = String::from_meta(&dtype).unwrap_or_else(
            |e| panic!("member {} has no valid datatype attribute: {}", ident_str, e));
        let dtype_expr = syn::parse_str::<Expr>(&dtype).unwrap_or_else(
            |_| panic!("member {} has no valid datatype attribute", ident_str));

        statics.push(quote! {
            static ref #dtype_static : datatype_type!(#dtype_expr) = #dtype_expr;
        });
        members.push(quote! { #ident, });
        member_to_json.push(quote! { #ident_str: #dtype_static.to_json(#ident)?, });
        member_contains.push(quote! { !obj.contains_key(#ident_str) });
        member_from_json.push(quote! { #ident: #dtype_static.from_json(&obj[#ident_str])?, });
        descr_members.push(quote! { #ident_str: #dtype_static.type_json(), });
        member_names.push(ident_str);
    }

    let generated = quote! {
        #vis struct #struct_name;

        #[allow(non_upper_case_globals)]
        const #const_name: () = {
            use serde_json::{json, Value};
            use lazy_static::lazy_static;
            use crate::errors::Error;

            lazy_static! {
                #( #statics )*
            }

            impl crate::types::TypeDesc for #struct_name {
                type Repr = #name;
                fn type_json(&self) -> Value {
                    json!(["struct", { #( #descr_members )* }])
                }
                fn to_json(&self, val: Self::Repr) -> std::result::Result<Value, Error> {
                    let #name { #( #members )* } = val;
                    Ok(json!({ #( #member_to_json )* }))
                }
                fn from_json(&self, val: &Value) -> std::result::Result<Self::Repr, Error> {
                    if let Some(obj) = val.as_object() {
                        #(
                            if #member_contains {
                                return Err(Error::bad_value(concat!("missing ", #member_names,
                                                                    " in object")));
                            }
                        )*
                        Ok(#name {
                            #( #member_from_json )*
                        })
                    } else {
                        Err(Error::bad_value("expected object"))
                    }
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
            use serde_json::{json, Value};
            use crate::errors::Error;

            impl crate::types::TypeDesc for #struct_name {
                type Repr = #name;
                fn type_json(&self) -> Value {
                    json!(["enum", { #( #descr_members )* }])
                }
                fn to_json(&self, val: Self::Repr) -> std::result::Result<Value, Error> {
                    Ok(json!(val as i64))
                }
                fn from_json(&self, val: &Value) -> std::result::Result<Self::Repr, Error> {
                    if let Some(s) = val.as_str() {
                        match s {
                            #( #str_arms )*
                            _ => Err(Error::bad_value("string not an enum member"))
                        }
                    } else if let Some(i) = val.as_i64() {
                        match i {
                            #( #int_arms )*
                            _ => Err(Error::bad_value("integer not an enum member"))
                        }
                    } else {
                        Err(Error::bad_value("expected string or integer"))
                    }
                }
            }
        };
    };
    // println!("{}", generated);
    generated
}
