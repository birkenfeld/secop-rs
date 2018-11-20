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
//! SECoP data type definitions.

use std::string::String as StdString;
use fxhash::FxHashMap as HashMap;
use serde_json::{Value, json};
use secop_derive::TypeDesc;

/// Represents a defined SECoP data type usable for parameters and command
/// arguments/results.
///
/// The Repr associated type should be set to a Rust type that can hold all
/// possible values, and conversion between JSON and that type is implemented
/// using `to_json` and `from_json`.
///
/// On conversion error, the incoming JSON Value is simply returned, and the
/// caller is responsible for raising the correct SECoP error.
pub trait TypeDesc {
    type Repr;
    /// Return a JSON-serialized description of the data type.
    fn type_json(&self) -> Value;
    /// Convert an internal value, as determined by the module code,
    /// into the JSON representation for the protocol.
    fn to_json(&self, val: Self::Repr) -> Result<Value, &'static str>;
    /// Convert an external JSON value, incoming from a connection.
    fn from_json(&self, val: &Value) -> Result<Self::Repr, &'static str>;
}


/// None is not usable as a parameter data type, only for commands.
pub struct None;

impl TypeDesc for None {
    type Repr = ();
    fn type_json(&self) -> Value { json!(null) }
    fn to_json(&self, _: Self::Repr) -> Result<Value, &'static str> {
        Ok(Value::Null)
    }
    fn from_json(&self, val: &Value) -> Result<Self::Repr, &'static str> {
        if val.is_null() { Ok(()) } else { Err("expected null") }
    }
}


pub struct Bool;

impl TypeDesc for Bool {
    type Repr = bool;
    fn type_json(&self) -> Value { json!(["bool"]) }
    fn to_json(&self, val: Self::Repr) -> Result<Value, &'static str> {
        Ok(Value::Bool(val))
    }
    fn from_json(&self, val: &Value) -> Result<Self::Repr, &'static str> {
        val.as_bool().ok_or("expected boolean")
    }
}


pub struct Double;

impl TypeDesc for Double {
    type Repr = f64;
    fn type_json(&self) -> Value { json!(["double"]) }
    fn to_json(&self, val: Self::Repr) -> Result<Value, &'static str> {
        Ok(json!(val))
    }
    fn from_json(&self, val: &Value) -> Result<Self::Repr, &'static str> {
        val.as_f64().ok_or("expected double")
    }
}


pub struct DoubleFrom(pub f64);

impl TypeDesc for DoubleFrom {
    type Repr = f64;
    fn type_json(&self) -> Value { json!(["double", self.0]) }
    fn to_json(&self, val: Self::Repr) -> Result<Value, &'static str> {
        Ok(json!(val))
    }
    fn from_json(&self, val: &Value) -> Result<Self::Repr, &'static str> {
        // TODO: check limits
        val.as_f64().ok_or("expected double")
    }
}


pub struct DoubleFromTo(pub f64, pub f64);

impl TypeDesc for DoubleFromTo {
    type Repr = f64;
    fn type_json(&self) -> Value { json!(["double", self.0, self.1]) }
    fn to_json(&self, val: Self::Repr) -> Result<Value, &'static str> {
        Ok(json!(val))
    }
    fn from_json(&self, val: &Value) -> Result<Self::Repr, &'static str> {
        // TODO: check limits
        val.as_f64().ok_or("expected double")
    }
}


pub struct Integer;

impl TypeDesc for Integer {
    type Repr = i64;
    fn type_json(&self) -> Value { json!(["int"]) }
    fn to_json(&self, val: Self::Repr) -> Result<Value, &'static str> {
        Ok(json!(val))
    }
    fn from_json(&self, val: &Value) -> Result<Self::Repr, &'static str> {
        val.as_i64().ok_or("expected integer")
    }
}


pub struct IntegerFrom(pub i64);

impl TypeDesc for IntegerFrom {
    type Repr = i64;
    fn type_json(&self) -> Value { json!(["int", self.0]) }
    fn to_json(&self, val: Self::Repr) -> Result<Value, &'static str> {
        Ok(json!(val))
    }
    fn from_json(&self, val: &Value) -> Result<Self::Repr, &'static str> {
        // TODO: check limits
        val.as_i64().ok_or("expected integer")
    }
}


pub struct IntegerFromTo(pub i64, pub i64);

impl TypeDesc for IntegerFromTo {
    type Repr = i64;
    fn type_json(&self) -> Value { json!(["int", self.0, self.1]) }
    fn to_json(&self, val: Self::Repr) -> Result<Value, &'static str> {
        Ok(json!(val))
    }
    fn from_json(&self, val: &Value) -> Result<Self::Repr, &'static str> {
        // TODO: check limits
        val.as_i64().ok_or("expected integer")
    }
}


pub struct Blob(pub usize, pub usize);

impl TypeDesc for Blob {
    type Repr = Vec<u8>;
    fn type_json(&self) -> Value { json!(["blob", self.0, self.1]) }
    fn to_json(&self, val: Self::Repr) -> Result<Value, &'static str> {
        Ok(Value::String(base64::encode(&val)))
    }
    fn from_json(&self, val: &Value) -> Result<Self::Repr, &'static str> {
        // TODO: check length limits
        val.as_str().and_then(|s| base64::decode(s).ok())
                    .ok_or("expected base64 coded string")
    }
}


pub struct String;

impl TypeDesc for String {
    type Repr = StdString;
    fn type_json(&self) -> Value { json!(["string"]) }
    fn to_json(&self, val: Self::Repr) -> Result<Value, &'static str> {
        Ok(Value::String(val))
    }
    fn from_json(&self, val: &Value) -> Result<Self::Repr, &'static str> {
        val.as_str().map(Into::into).ok_or("expected string")
    }
}


pub struct StringUpto(pub usize);

impl TypeDesc for StringUpto {
    type Repr = StdString;
    fn type_json(&self) -> Value { json!(["string", 0, self.0]) }
    fn to_json(&self, val: Self::Repr) -> Result<Value, &'static str> {
        Ok(Value::String(val))
    }
    fn from_json(&self, val: &Value) -> Result<Self::Repr, &'static str> {
        // TODO: check length limits
        val.as_str().map(Into::into).ok_or("expected string")
    }
}


/// A generic enum.  On the Rust side, this is represented as an untyped i64.
///
/// You should prefer implementing your own enum class and deriving `TypeDesc`
/// for it using secop-derive.
pub struct Enum(pub HashMap<StdString, i64>);

impl TypeDesc for Enum {
    type Repr = i64;
    fn type_json(&self) -> Value {
        json!(["enum", self.0])
    }
    fn to_json(&self, val: Self::Repr) -> Result<Value, &'static str> {
        Ok(json!(val))
    }
    fn from_json(&self, val: &Value) -> Result<Self::Repr, &'static str> {
        if let Some(s) = val.as_str() {
            self.0.get(s).cloned().ok_or("string not an enum member")
        } else if let Some(i) = val.as_i64() {
            if self.0.values().any(|&j| i == j) { Ok(i) }
            else { Err("integer not an enum member") }
        } else {
            Err("expected string or integer")
        }
    }
}


pub struct ArrayOf<T: TypeDesc>(pub T);

impl<T: TypeDesc> TypeDesc for ArrayOf<T> {
    type Repr = Vec<T::Repr>;
    fn type_json(&self) -> Value {
        json!(["array", self.0.type_json()])
    }
    fn to_json(&self, val: Self::Repr) -> Result<Value, &'static str> {
        let v: Result<Vec<_>, _> = val.into_iter().map(|v| self.0.to_json(v)).collect();
        Ok(Value::Array(v?))
    }
    fn from_json(&self, val: &Value) -> Result<Self::Repr, &'static str> {
        if let Some(arr) = val.as_array() {
            arr.iter().map(|v| self.0.from_json(v)).collect()
        } else {
            Err("expected array")
        }
    }
}


pub struct ArrayOfUpto<T: TypeDesc>(pub T, pub usize);

impl<T: TypeDesc> TypeDesc for ArrayOfUpto<T> {
    type Repr = Vec<T::Repr>;
    fn type_json(&self) -> Value {
        json!(["array", self.0.type_json(), 0, self.1])
    }
    fn to_json(&self, val: Self::Repr) -> Result<Value, &'static str> {
        let v: Result<Vec<_>, _> = val.into_iter().map(|v| self.0.to_json(v)).collect();
        Ok(Value::Array(v?))
    }
    fn from_json(&self, val: &Value) -> Result<Self::Repr, &'static str> {
        // TODO: check length limits
        if let Some(arr) = val.as_array() {
            arr.iter().map(|v| self.0.from_json(v)).collect()
        } else {
            Err("expected array")
        }
    }
}


macro_rules! impl_tuple {
    ($name:tt => $($tv:tt),* : $len:tt : $($idx:tt),*) => {
        pub struct $name<$($tv: TypeDesc),*>($(pub $tv),*);

        impl<$($tv: TypeDesc),*> TypeDesc for $name<$($tv),*> {
            type Repr = ($($tv::Repr),*);
            fn type_json(&self) -> Value {
                json!(["tuple", $(self.$idx.type_json()),*])
            }
            fn to_json(&self, val: Self::Repr) -> Result<Value, &'static str> {
                Ok(json!([ $(self.$idx.to_json(val.$idx)),* ]))
            }
            fn from_json(&self, val: &Value) -> Result<Self::Repr, &'static str> {
                if let Some(arr) = val.as_array() {
                    if arr.len() == $len {
                        return Ok((
                            $(self.$idx.from_json(&arr[$idx])?),*
                        ));
                    }
                }
                Err(concat!("expected array with ", stringify!($len), " elements"))
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


// Helpers for easy Enum creation

impl Enum {
    pub fn new() -> Enum {
        Enum(HashMap::default())
    }

    pub fn add(mut self, name: &str) -> Self {
        let n = self.0.len() as i64;
        self.0.insert(name.into(), n);
        self
    }

    pub fn insert(mut self, name: &str, value: i64) -> Self {
        self.0.insert(name.into(), value);
        self
    }
}

// The Status enum, and predefined type.

#[derive(TypeDesc, Clone, Copy, PartialEq, Eq, Hash)]
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
pub type StatusType = Tuple2<StatusConstType, String>;
#[allow(non_upper_case_globals)]
pub const StatusType: StatusType = Tuple2(StatusConstType, String);

pub type Status = (StatusConst, StdString);


// This is a bit of a mess :(
//
// In order to generate the param/cmd types as statics, we need not only
// the value given by the user, e.g. `DoubleFrom(0.)`, but also its type
// (since statics don't allow inferring the type).
//
// This provides a working but very brittle way of doing this, for now.
macro_rules! datatype_type {
    (DoubleFrom($_:expr)) => (DoubleFrom);
    (DoubleFromTo($_:expr, $__:expr)) => (DoubleFromTo);
    (IntegerFrom($_:expr)) => (IntegerFrom);
    (IntegerFromTo($_:expr)) => (IntegerFromTo);
    (Blob($_:expr)) => (Blob);
    (StringUpto($_:expr)) => (StringUpto);
    (Enum $($_:tt)*) => (Enum);
    (ArrayOf($($tp:tt)*)) => (ArrayOf<datatype_type!($($tp)*)>);
    (ArrayOfUpto($($tp:tt)*, $($_:tt)*)) => (ArrayOfUpto<datatype_type!($($tp)*)>);
    // For "simple" (unit-struct) types, which includes user-derived types.
    ($stalone_type:ty) => ($stalone_type);
}
