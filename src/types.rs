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
//! SeCOP data type definitions.

use std::borrow::Cow;
use std::collections::HashMap;
use serde_json::Value;
use serde::{Serialize, Serializer, ser::SerializeSeq};
use serde_derive::Serialize;

// use errors::Error;

pub type Lim<T> = Option<(T, T)>;

pub enum TypeDesc {
    Bool,
    Double(Lim<f64>),
    Integer(Lim<f64>),
    Blob(Lim<usize>),
    String(Lim<usize>),
    Enum(HashMap<String, Value>),
    ArrayOf(Box<TypeDesc>, Lim<usize>),
    TupleOf(Vec<TypeDesc>),
    StructOf(HashMap<String, TypeDesc>),
    Command(Box<TypeDesc>, Box<TypeDesc>),
}

// impl TypeDesc {
//     pub fn validate(&self, value: &Value) -> Result<(), Error> {
//         match self {
//             TypeDesc::Bool => value.as_bool()
//         }
//     }
// }

impl Serialize for TypeDesc {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut seq = s.serialize_seq(None)?;
        match self {
            TypeDesc::Bool => seq.serialize_element("bool")?,
            TypeDesc::Double(lim) => {
                seq.serialize_element("double")?;
                serialize_limit(&mut seq, lim)?;
            }
            TypeDesc::Integer(lim) => {
                seq.serialize_element("int")?;
                serialize_limit(&mut seq, lim)?;
            }
            TypeDesc::Blob(lim) => {
                seq.serialize_element("blob")?;
                serialize_limit(&mut seq, lim)?;
            }
            TypeDesc::String(lim) => {
                seq.serialize_element("string")?;
                serialize_limit(&mut seq, lim)?;
            }
            TypeDesc::Enum(values) => {
                seq.serialize_element("enum")?;
                seq.serialize_element(values)?;
            }
            TypeDesc::ArrayOf(subtype, lim) => {
                seq.serialize_element("array")?;
                seq.serialize_element(subtype)?;
                serialize_limit(&mut seq, lim)?;
            }
            TypeDesc::TupleOf(subtypes) => {
                seq.serialize_element("tuple")?;
                seq.serialize_element(subtypes)?;
            }
            TypeDesc::StructOf(subtypes) => {
                seq.serialize_element("struct")?;
                seq.serialize_element(subtypes)?;
            }
            TypeDesc::Command(argtype, restype) => {
                seq.serialize_element("command")?;
                seq.serialize_element(argtype)?;
                seq.serialize_element(restype)?;
            }
        }
        seq.end()
    }
}

fn serialize_limit<S: SerializeSeq, T: Serialize>(seq: &mut S, lim: &Lim<T>) -> Result<(), S::Error> {
    if let Some((min, max)) = lim {
        seq.serialize_element(&min)?;
        seq.serialize_element(&max)?;
    }
    Ok(())
}

// Descriptive Data hierarchy


pub type Str<'a> = Cow<'a, str>;

#[derive(Serialize)]
pub struct NodeDesc<'a> {
    modules: Vec<(Str<'a>, ModuleDesc<'a>)>,
    equipment_id: Str<'a>,
    firmware: Str<'a>,
    version: Str<'a>,
}

#[derive(Serialize)]
pub struct ModuleDesc<'a> {
    accessibles: Vec<(String, AccessibleDesc<'a>)>,
    properties: HashMap<Str<'a>, Str<'a>>,
}

#[derive(Serialize)]
pub struct AccessibleDesc<'a> {
    description: Str<'a>,
    datatype: TypeDesc,
    #[serde(skip_serializing_if = "Option::is_none")]
    readonly: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    unit: Option<Str<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    group: Option<Str<'a>>,
}
