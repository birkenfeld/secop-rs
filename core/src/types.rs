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
//   Enrico Faulhaber <enrico.faulhaber@frm2.tum.de>
//
// -----------------------------------------------------------------------------
//
//! SECoP data type / data info definitions.

use std::collections::HashMap;
use serde::ser::{Serialize, Serializer, SerializeMap};
use serde_derive::Serialize;
use serde_json::{Value, json};
use secop_derive::TypeInfo;

use crate::errors::Error;

fn is_zero(v: &usize) -> bool { *v == 0 }
fn is_false(v: &bool) -> bool { !*v }
fn is_none<T>(v: &Option<T>) -> bool { v.is_none() }


/// Represents a defined SECoP data type with meta information usable for
/// parameters and command arguments/results.
///
/// The Repr associated type should be set to a Rust type that can hold all
/// possible values, and conversion between JSON and that type is implemented
/// using `to_json` and `from_json`.
///
/// On conversion error, the incoming JSON Value is simply returned, and the
/// caller is responsible for raising the correct SECoP error.
pub trait TypeInfo : Serialize {
    type Repr;
    /// Convert an internal value, as determined by the module code,
    /// into the JSON representation for the protocol.
    fn to_json(&self, val: Self::Repr) -> Result<Value, Error>;
    /// Convert an external JSON value, incoming from a connection.
    fn from_json(&self, val: &Value) -> Result<Self::Repr, Error>;
}


/// Null is not usable as a parameter data type, only for commands.
pub struct Null;

impl Serialize for Null {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where S: Serializer
    {
        serializer.serialize_none()
    }
}

impl TypeInfo for Null {
    type Repr = ();

    fn to_json(&self, _: Self::Repr) -> Result<Value, Error> {
        Ok(Value::Null)
    }

    fn from_json(&self, val: &Value) -> Result<Self::Repr, Error> {
        if val.is_null() { Ok(()) } else { Err(Error::bad_value("expected null")) }
    }
}


pub struct Bool;

impl Serialize for Bool {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where S: Serializer
    {
        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry("type", "bool")?;
        map.end()
    }
}

impl TypeInfo for Bool {
    type Repr = bool;

    fn to_json(&self, val: Self::Repr) -> Result<Value, Error> {
        Ok(Value::Bool(val))
    }

    fn from_json(&self, val: &Value) -> Result<Self::Repr, Error> {
        val.as_bool().ok_or_else(|| Error::bad_value("expected boolean"))
    }
}


#[derive(Serialize)]
#[serde(tag = "type", rename = "double")]
pub struct Double {
    #[serde(skip_serializing_if = "is_none")]
    min: Option<f64>,
    #[serde(skip_serializing_if = "is_none")]
    max: Option<f64>,
    #[serde(skip_serializing_if = "is_none")]
    unit: Option<String>,  // TODO: interning?
    #[serde(skip_serializing_if = "is_none")]
    fmtstr: Option<String>,
    #[serde(skip_serializing_if = "is_none")]
    absolute_resolution: Option<f64>,
    #[serde(skip_serializing_if = "is_none")]
    relative_resolution: Option<f64>,
}

impl Double {
    pub fn new() -> Self {
        Self { min: None, max: None, unit: None, fmtstr: None,
               absolute_resolution: None, relative_resolution: None }
    }

    pub fn min(self, val: f64) -> Self {
        Self { min: Some(val), .. self }
    }

    pub fn max(self, val: f64) -> Self {
        Self { max: Some(val), .. self }
    }

    pub fn unit(self, val: &str) -> Self {
        Self { unit: Some(val.into()), .. self }
    }

    pub fn fmtstr(self, val: &str) -> Self {
        Self { fmtstr: Some(val.into()), .. self }
    }

    pub fn absolute_resolution(self, val: f64) -> Self {
        Self { absolute_resolution: Some(val), .. self }
    }

    pub fn relative_resolution(self, val: f64) -> Self {
        Self { relative_resolution: Some(val), .. self }
    }
}

impl Double {
    fn check(&self, v: f64) -> Result<bool, Error> {
        match (self.min, self.max) {
            (Some(min), Some(max)) => if v < min || v > max {
                return Err(Error::bad_value(format!("expected double between {} and {}", min, max)));
            }
            (Some(min), _) => if v < min {
                return Err(Error::bad_value(format!("expected double above {}", min)));
            }
            (_, Some(max)) => if v > max {
                return Err(Error::bad_value(format!("expected double below {}", max)));
            }
            _ => ()
        }
        Ok(true)
    }
}

impl TypeInfo for Double {
    type Repr = f64;

    fn to_json(&self, val: Self::Repr) -> Result<Value, Error> {
        self.check(val)?;
        Ok(json!(val))
    }

    fn from_json(&self, val: &Value) -> Result<Self::Repr, Error> {
        match val.as_f64() {
            Some(v) if self.check(v)? => Ok(v),
            _ => Err(Error::bad_value(format!("expected double")))
        }
    }
}


#[derive(Serialize)]
#[serde(tag = "type", rename = "scaled")]
pub struct Scaled {
    scale: f64,
    min: i64,
    max: i64,
    #[serde(skip_serializing_if = "is_none")]
    unit: Option<String>,  // TODO: interning?
    #[serde(skip_serializing_if = "is_none")]
    fmtstr: Option<String>,
    #[serde(skip_serializing_if = "is_none")]
    absolute_resolution: Option<f64>,
    #[serde(skip_serializing_if = "is_none")]
    relative_resolution: Option<f64>,
}

impl Scaled {
    pub fn new() -> Self {
        Self { scale: 1.0, min: i64::MIN, max: i64::MAX, unit: None, fmtstr: None,
               absolute_resolution: None, relative_resolution: None }
    }

    pub fn scale(self, val: f64) -> Self {
        Self { scale: val, .. self }
    }

    pub fn min(self, val: i64) -> Self {
        Self { min: val, .. self }
    }

    pub fn max(self, val: i64) -> Self {
        Self { max: val, .. self }
    }

    pub fn unit(self, val: &str) -> Self {
        Self { unit: Some(val.into()), .. self }
    }

    pub fn fmtstr(self, val: &str) -> Self {
        Self { fmtstr: Some(val.into()), .. self }
    }

    pub fn absolute_resolution(self, val: f64) -> Self {
        Self { absolute_resolution: Some(val), .. self }
    }

    pub fn relative_resolution(self, val: f64) -> Self {
        Self { relative_resolution: Some(val), .. self }
    }
}

impl TypeInfo for Scaled {
    type Repr = f64;

    fn to_json(&self, val: Self::Repr) -> Result<Value, Error> {
        let val = (val / self.scale).round() as i64;
        if val >= self.min && val <= self.max {
            Ok(json!(val))
        } else {
            Err(Error::bad_value(format!("expected double between {} and {}",
                                         self.scale * self.min as f64,
                                         self.scale * self.max as f64)))
        }
    }

    fn from_json(&self, val: &Value) -> Result<Self::Repr, Error> {
        match val.as_i64() {
            Some(v) if v >= self.min && v <= self.max => Ok(v as f64 * self.scale),
            _ => Err(Error::bad_value(format!("expected integer between {} and {}",
                                              self.min, self.max)))
        }
    }
}


#[derive(Serialize)]
#[serde(tag = "type", rename = "int")]
pub struct Int {
    min: i64,
    max: i64,
}

impl Int {
    pub fn new() -> Self {
        Self { min: i64::MIN, max: i64::MAX }
    }

    pub fn min(self, val: i64) -> Self {
        Self { min: val, .. self }
    }

    pub fn max(self, val: i64) -> Self {
        Self { max: val, .. self }
    }
}

impl TypeInfo for Int {
    type Repr = i64;

    fn to_json(&self, val: Self::Repr) -> Result<Value, Error> {
        if val >= self.min && val <= self.max {
            Ok(json!(val))
        } else {
            Err(Error::bad_value(format!("expected integer between {} and {}",
                                         self.min, self.max)))
        }
    }

    fn from_json(&self, val: &Value) -> Result<Self::Repr, Error> {
        match val.as_i64() {
            Some(v) if v >= self.min && v <= self.max => Ok(v),
            _ => Err(Error::bad_value(format!("expected integer between {} and {}",
                                              self.min, self.max)))
        }
    }
}


#[derive(Serialize)]
#[serde(tag = "type", rename = "blob")]
pub struct Blob {
    #[serde(skip_serializing_if = "is_zero")]
    minbytes: usize,
    maxbytes: usize,
}

impl Blob {
    pub fn new() -> Self {
        Self { minbytes: 0, maxbytes: 1024 }
    }

    pub fn minbytes(self, val: usize) -> Self {
        Self { minbytes: val, .. self }
    }

    pub fn maxbytes(self, val: usize) -> Self {
        Self { maxbytes: val, .. self }
    }
}

impl TypeInfo for Blob {
    type Repr = Vec<u8>;

    fn to_json(&self, val: Self::Repr) -> Result<Value, Error> {
        if val.len() >= self.minbytes && val.len() <= self.maxbytes {
            Ok(Value::String(base64::encode(&val)))
        } else {
            Err(Error::bad_value(format!("expected blob with length between {} and {}",
                                         self.minbytes, self.maxbytes)))
        }
    }

    fn from_json(&self, val: &Value) -> Result<Self::Repr, Error> {
        if let Some(v) = val.as_str().and_then(|s| base64::decode(s).ok()) {
            if v.len() >= self.minbytes && v.len() <= self.maxbytes {
                return Ok(v);
            }
        }
        Err(Error::bad_value(format!("expected base64 coded string with decoded \
                                      length between {} and {}", self.minbytes, self.maxbytes)))
    }
}


#[derive(Serialize)]
#[serde(tag = "type", rename = "string")]
pub struct Str {
    #[serde(skip_serializing_if = "is_zero")]
    minchars: usize,
    maxchars: usize,
    #[serde(rename = "isUTF8")]
    #[serde(skip_serializing_if = "is_false")]
    is_utf8: bool,
}

impl Str {
    pub fn new() -> Self {
        Self { minchars: 0, maxchars: 1024, is_utf8: false }
    }

    pub fn minchars(self, val: usize) -> Self {
        Self { minchars: val, .. self }
    }

    pub fn maxchars(self, val: usize) -> Self {
        Self { maxchars: val, .. self }
    }

    pub fn is_utf8(self, val: bool) -> Self {
        Self { is_utf8: val, .. self }
    }
}

impl TypeInfo for Str {
    type Repr = String;

    fn to_json(&self, val: Self::Repr) -> Result<Value, Error> {
        if val.len() <= self.maxchars {
            Ok(Value::String(val))
        } else {
            Err(Error::bad_value(format!("expected string with length <= {:?}", self.maxchars)))
        }
    }

    fn from_json(&self, val: &Value) -> Result<Self::Repr, Error> {
        match val.as_str() {
            Some(v) if v.len() <= self.maxchars => Ok(v.into()),
            _ => Err(Error::bad_value(format!("expected string with length <= {:?}", self.maxchars)))
        }
    }
}


#[derive(Serialize)]
#[serde(tag = "type", rename = "array")]
pub struct ArrayOf<T: TypeInfo> {
    #[serde(skip_serializing_if = "is_zero")]
    pub minlen: usize,
    pub maxlen: usize,
    pub members: T,
}

impl<T: TypeInfo> TypeInfo for ArrayOf<T> {
    type Repr = Vec<T::Repr>;

    fn to_json(&self, val: Self::Repr) -> Result<Value, Error> {
        if val.len() >= self.minlen && val.len() <= self.maxlen {
            let v: Result<Vec<_>, _> = val.into_iter().enumerate().map(|(i, v)| {
                self.members.to_json(v).map_err(|e| e.amend(&format!("in item {}", i+1)))
            }).collect();
            Ok(Value::Array(v?))
        } else {
            Err(Error::bad_value(format!("expected vector with length between {} and {}",
                                         self.minlen, self.maxlen)))
        }
    }

    fn from_json(&self, val: &Value) -> Result<Self::Repr, Error> {
        match val.as_array() {
            Some(arr) if arr.len() >= self.minlen && arr.len() <= self.maxlen => {
                arr.iter().enumerate().map(|(i, v)| {
                    self.members.from_json(v).map_err(|e| e.amend(&format!("in item {}", i+1)))
                }).collect()
            }
            _ => Err(Error::bad_value(format!("expected array with length between {} and {}",
                                              self.minlen, self.maxlen)))
        }
    }
}


macro_rules! impl_tuple {
    ($name:tt => $($tv:tt),* : $len:tt : $($idx:tt),*) => {
        pub struct $name<$($tv: TypeInfo),*>($(pub $tv),*);

        impl<$($tv: TypeInfo),*> Serialize for $name<$($tv),*> {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where S: Serializer {
                let members = ($(&self.$idx),+);
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "tuple")?;
                map.serialize_entry("members", &members)?;
                map.end()
            }
        }

        impl<$($tv: TypeInfo),*> TypeInfo for $name<$($tv),*> {
            type Repr = ($($tv::Repr),*);

            fn to_json(&self, val: Self::Repr) -> Result<Value, Error> {
                Ok(json!([ $(
                    self.$idx.to_json(val.$idx)
                             .map_err(|e| e.amend(concat!("in item ", $idx))) ?
                ),* ]))
            }

            fn from_json(&self, val: &Value) -> Result<Self::Repr, Error> {
                if let Some(arr) = val.as_array() {
                    if arr.len() == $len {
                        return Ok((
                            $(
                                self.$idx.from_json(&arr[$idx])
                                         .map_err(|e| e.amend(concat!("in item ", $idx))) ?
                            ),*
                        ));
                    }
                }
                Err(Error::bad_value(concat!("expected array with ",
                                             stringify!($len), " elements")))
            }
        }
    }
}

impl_tuple!(Tuple2 => T1, T2 : 2 : 0, 1);
impl_tuple!(Tuple3 => T1, T2, T3 : 3 : 0, 1, 2);
impl_tuple!(Tuple4 => T1, T2, T3, T4 : 4 : 0, 1, 2, 3);
impl_tuple!(Tuple5 => T1, T2, T3, T4, T5 : 5 : 0, 1, 2, 3, 4);
impl_tuple!(Tuple6 => T1, T2, T3, T4, T5, T6 : 6 : 0, 1, 2, 3, 4, 5);

// Note: There is no type for Command, since it's only a pseudo-type that
// is not actually validated/converted to.


/// A generic enum.  On the Rust side, this is represented as an untyped i64.
///
/// You should prefer implementing your own enum class and deriving `TypeInfo`
/// for it using secop-derive.
#[derive(Serialize)]
#[serde(tag = "type", rename = "enum")]
pub struct Enum {
    members: HashMap<String, i64>
}

impl TypeInfo for Enum {
    type Repr = i64;

    fn to_json(&self, val: Self::Repr) -> Result<Value, Error> {
        if self.members.values().any(|&j| val == j) {
            Ok(json!(val))
        } else {
            Err(Error::bad_value("integer not an enum member"))
        }
    }

    fn from_json(&self, val: &Value) -> Result<Self::Repr, Error> {
        if let Some(s) = val.as_str() {
            self.members.get(s).cloned().ok_or_else(|| Error::bad_value("string not an enum member"))
        } else if let Some(i) = val.as_i64() {
            if self.members.values().any(|&j| i == j) { Ok(i) }
            else { Err(Error::bad_value("integer not an enum member")) }
        } else {
            Err(Error::bad_value("expected string or integer"))
        }
    }
}


// The Status enum, and predefined type.

#[derive(TypeInfo, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StatusConst {
    Idle = 100,
    Warn = 200,
    Unstable = 250,
    Busy = 300,
    Error = 400,
    Unknown = 500,
}

impl Default for StatusConst {
    fn default() -> Self {
        StatusConst::Idle
    }
}

// This could also be a new unit-struct type, but it works as a type
// alias as well, with less code duplication.  But we need both the
// type alias and the value alias.
//
// This only looks confusing unless you realize that for unit-structs,
// there is *always* a constant with the same name as the type.
pub type StatusType = Tuple2<StatusConstType, Str>;
#[allow(non_upper_case_globals)]
pub const StatusType: StatusType = Tuple2(StatusConstType, Str { minchars: 0, maxchars: 1024, is_utf8: false });

pub type Status = (StatusConst, String);
