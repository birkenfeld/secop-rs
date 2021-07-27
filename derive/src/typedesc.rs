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
//!
//!
//! Deriving this for Rust enums (which can only be C-like enums) results in a
//! SECoP datatype describing that enum.  Integer discriminants for individual
//! variants are taken into account.
//!
//! Deriving this for Rust structs results in a SECoP struct datatype.  The
//! SECoP datatype for each struct member must be selected with another
//! attribute.
//!
//! This derive is a bit special because it generates not an `impl` for the
//! actual type, but a separate type (the "metatype") and an impl for that.
//!
//! ## Enum example
//!
//! This declaration:
//!
//! ```
//! #[derive(TypeDesc, Clone, PartialEq)]
//! enum Mode {
//!     PID,
//!     Ramp,
//!     OpenLoop = 10,
//! }
//! ```
//!
//! will result in a new struct called `ModeType` which implements `TypeDesc`.  The
//! respective SECoP enum will have value names "PID", "Ramp", and "OpenLoop"
//! with values 0, 1 and 10.  For example:
//!
//! ```
//! #[param(name="mode", datatype="ModeType", default="Mode::PID")]
//! struct Controller { ... }
//!
//! impl Controller {
//!     fn read_mode(&mut self) -> Result<Mode> { ... }
//!     fn write_mode(&mut self, mode: Mode) -> Result<()> { ... }
//! }
//! ```
//!
//! As you can see, `ModeType` is used as the `datatype` attribute for
//! parameters of modules, while the original `Mode` type is what the `read_`
//! and `write_` methods deal with.
//!
//! ## Struct example
//!
//! An example struct:
//!
//! ```
//! #[derive(TypeDesc, Clone, PartialEq)]
//! struct PID {
//!     #[datatype="DoubleFrom(0.0)"]
//!     p: f64,
//!     #[datatype="DoubleFrom(0.0)"]
//!     i: f64,
//!     #[datatype="DoubleFrom(0.0)"]
//!     d: f64,
//! }
//! ```
//!
//! The newly created `PIDType` describes a SECoP struct with members
//! "p", "i" and "d", and the `PID` struct itself is used to pass values
//! of this type to the internal methods.


use syn::Expr;
use quote::{quote, format_ident};
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
    // Wrapping the derive code in a "const" namespace is a trick that gives us
    // an enclosing scope where we can `use` things without leaking, and Rust does
    // not care that the actual impls are inside it.  Only the `struct` definition
    // itself must be outside.
    let const_name = format_ident!("_DERIVE_TypeDesc_{}", name);
    let struct_name = format_ident!("{}Type", name);

    let mut statics = Vec::new();
    let mut member_to_json = Vec::new();
    let mut member_from_json = Vec::new();
    let mut descr_members = Vec::new();
    let mut descr_optional = Vec::new();

    // Go through each field, and construct the SECoP metatype for it.
    for binding in input.variants()[0].bindings() {
        // We could support tuple structs to work the same as tuples, but it
        // seems unnecessary at the moment.
        let ident = binding.ast().ident.as_ref().unwrap_or_else(
            || panic!("TypeDesc cannot be derived for tuple structs"));
        let ident_str = ident.to_string();
        // We need the metatype instance globally available somewhere.  Since
        // it cannot (currently) be constructed in a `const` context, it needs
        // to be a lazy static.
        let dtype_static = format_ident!("STRUCT_FIELD_{}", ident_str);
        // Find the `datatype` attribute on the field.  It must be present.
        let mut dtype = None;
        for attr in &binding.ast().attrs {
            if attr.path.segments[0].ident == "datatype" {
                dtype = attr.parse_meta().ok();
            }
        }
        let dtype = dtype.unwrap_or_else(
            || panic!("member {} has no valid datatype attribute", ident_str));
        let dtype = String::from_meta(&dtype).unwrap_or_else(
            |e| panic!("member {} has no valid datatype attribute: {}", ident_str, e));
        let dtype_expr = syn::parse_str::<Expr>(&dtype).unwrap_or_else(
            |_| panic!("member {} has no valid datatype attribute", ident_str));
        // Check if the Rust type is an Option.
        let mut is_option_type = false;
        if let syn::Type::Path(ref ptype) = binding.ast().ty {
            if ptype.path.segments[0].ident == "Option" {
                is_option_type = true;
            }
        }

        statics.push(quote! {
            static ref #dtype_static: typedesc_type!(#dtype_expr) = #dtype_expr;
        });

        // Other code snippets we need to construct the TypeDesc impl.
        if is_option_type {
            // Option<T>: value missing from JSON <=> value is None.
            member_to_json.push(quote! {
                if let Some(member) = val.#ident {
                    // Note the `amend`, in order to try making the error easier to
                    // pinpoint in complex data structures.
                    let json_member = #dtype_static.to_json(member)
                                                   .map_err(|e| e.amend(concat!("in ", #ident_str)))?;
                    map.insert(#ident_str.into(), json_member);
                }
            });
            member_from_json.push(quote! {
                #ident: match obj.get(#ident_str) {
                    None => None,
                    Some(val) => Some(#dtype_static.from_json(val)
                                      .map_err(|e| e.amend(concat!("in ", #ident_str)))?),
                },
            });
        } else {
            // Non-option type: value *must* be present in JSON.
            member_to_json.push(quote! {
                let json_member = #dtype_static.to_json(val.#ident)
                                               .map_err(|e| e.amend(concat!("in ", #ident_str)))?;
                map.insert(#ident_str.into(), json_member);
            });
            member_from_json.push(quote! {
                #ident: #dtype_static.from_json(
                    obj.get(#ident_str).ok_or_else(
                        || Error::bad_value(concat!("missing ", #ident_str, " in object")))?
                ).map_err(|e| e.amend(concat!("in ", #ident_str)))?,
            });
        }
        descr_members.push(quote! { #ident_str: #dtype_static.type_json(), });
        descr_optional.push(quote! { #ident_str, });
    }

    let generated = quote! {
        #vis struct #struct_name;

        #[allow(non_upper_case_globals)]
        const #const_name: () = {
            use serde_json::{json, Value, map::Map};
            use lazy_static::lazy_static;
            use crate::secop_core::errors::Error;
            use crate::secop_core::types::TypeDesc;

            lazy_static! {
                #( #statics )*
            }

            impl TypeDesc for #struct_name {
                type Repr = #name;
                fn type_json(&self) -> Value {
                    json!({"type": "struct",
                           "members": { #( #descr_members )* },
                           "optional": [ #( #descr_optional )* ]})
                }
                fn to_json(&self, val: Self::Repr) -> std::result::Result<Value, Error> {
                    let mut map = Map::new();
                    #( #member_to_json )*
                    Ok(Value::Object(map))
                }
                fn from_json(&self, val: &Value) -> std::result::Result<Self::Repr, Error> {
                    if let Some(obj) = val.as_object() {
                        Ok(#name { #( #member_from_json )* })
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
    let const_name = format_ident!("_DERIVE_TypeDesc_{}", name);
    let struct_name = format_ident!("{}Type", name);

    let mut descr_members = Vec::new();
    let mut str_arms = Vec::new();
    let mut int_arms = Vec::new();

    let mut discr = -1i64;
    for variant in input.variants() {
        let ident = &variant.ast().ident;
        let ident_str = ident.to_string();
        if variant.ast().fields != &syn::Fields::Unit {
            panic!("enum member {} cannot have data associated with it", ident);
        }
        if let Some((_, dis)) = variant.ast().discriminant {
            if let syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Int(i), .. }) = dis {
                discr = i.base10_parse().unwrap();
            } else {
                panic!("explicit enum discriminants can only be integer literals");
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
            use crate::secop_core::errors::Error;
            use crate::secop_core::types::TypeDesc;

            impl TypeDesc for #struct_name {
                type Repr = #name;
                fn type_json(&self) -> Value {
                    json!({"type": "enum", "members": { #( #descr_members )* }})
                }
                fn to_json(&self, val: Self::Repr) -> std::result::Result<Value, Error> {
                    Ok(json!(val as i64))
                }
                fn from_json(&self, val: &Value) -> std::result::Result<Self::Repr, Error> {
                    if let Some(s) = val.as_str() {
                        match s {
                            #( #str_arms )*
                            _ => Err(Error::bad_value(
                                format!("{:?} is not an enum member", s)))
                        }
                    } else if let Some(i) = val.as_i64() {
                        match i {
                            #( #int_arms )*
                            _ => Err(Error::bad_value(
                                format!("{:?} is not an enum member", i)))
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
