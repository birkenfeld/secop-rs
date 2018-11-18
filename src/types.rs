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
//! SeCOP data type definitions.

use std::string::String as StdString;
use fxhash::FxHashMap as HashMap;
use serde_json::Value;

/// Represents a defined SeCOP data type usable for parameters and command
/// arguments/results.
///
/// The Repr associated type should be set to a Rust type that can hold all
/// possible values, and conversion between JSON and that type is implemented
/// using `from_repr` and `to_repr`.
///
/// On conversion error, the incoming JSON Value is simply returned, and the
/// caller is responsible for raising the correct SeCOP error.
pub trait TypeDesc {
    type Repr;
    /// Return a JSON-serialized description of the data type.
    fn as_json(&self) -> Value;
    /// Convert an internal value, as determined by the module code,
    /// into the JSON representation for the protocol.
    fn from_repr(&self, val: Self::Repr) -> Value;
    /// Convert an external JSON value, incoming from a connection.
    fn to_repr(&self, val: Value) -> Result<Self::Repr, Value>;
}


/// None is not usable as a parameter data type, only for commands.
pub struct None;

impl TypeDesc for None {
    type Repr = ();
    fn as_json(&self) -> Value { json!(null) }
    fn from_repr(&self, _: Self::Repr) -> Value { Value::Null }
    fn to_repr(&self, val: Value) -> Result<Self::Repr, Value> {
        if val.is_null() { Ok(()) } else { Err(val) }
    }
}


pub struct Bool;

impl TypeDesc for Bool {
    type Repr = bool;
    fn as_json(&self) -> Value { json!(["bool"]) }
    fn from_repr(&self, val: Self::Repr) -> Value { json!(val) }
    fn to_repr(&self, val: Value) -> Result<Self::Repr, Value> {
        val.as_bool().ok_or(val)
    }
}


pub struct Double;

impl TypeDesc for Double {
    type Repr = f64;
    fn as_json(&self) -> Value { json!(["double"]) }
    fn from_repr(&self, val: Self::Repr) -> Value { json!(val) }
    fn to_repr(&self, val: Value) -> Result<Self::Repr, Value> {
        val.as_f64().ok_or(val)
    }
}


pub struct DoubleFrom(pub f64);

impl TypeDesc for DoubleFrom {
    type Repr = f64;
    fn as_json(&self) -> Value { json!(["double", self.0]) }
    fn from_repr(&self, val: Self::Repr) -> Value { json!(val) }
    fn to_repr(&self, val: Value) -> Result<Self::Repr, Value> {
        val.as_f64().ok_or(val)
    }
}


pub struct DoubleFromTo(pub f64, pub f64);

impl TypeDesc for DoubleFromTo {
    type Repr = f64;
    fn as_json(&self) -> Value { json!(["double", self.0, self.1]) }
    fn from_repr(&self, val: Self::Repr) -> Value { json!(val) }
    fn to_repr(&self, val: Value) -> Result<Self::Repr, Value> {
        val.as_f64().ok_or(val)
    }
}


pub struct Integer;

impl TypeDesc for Integer {
    type Repr = i64;
    fn as_json(&self) -> Value { json!(["int"]) }
    fn from_repr(&self, val: Self::Repr) -> Value { json!(val) }
    fn to_repr(&self, val: Value) -> Result<Self::Repr, Value> {
        val.as_i64().ok_or(val)
    }
}


pub struct IntegerFrom(pub i64);

impl TypeDesc for IntegerFrom {
    type Repr = i64;
    fn as_json(&self) -> Value { json!(["int", self.0]) }
    fn from_repr(&self, val: Self::Repr) -> Value { json!(val) }
    fn to_repr(&self, val: Value) -> Result<Self::Repr, Value> {
        val.as_i64().ok_or(val)
    }
}


pub struct IntegerFromTo(pub i64, pub i64);

impl TypeDesc for IntegerFromTo {
    type Repr = i64;
    fn as_json(&self) -> Value { json!(["int", self.0, self.1]) }
    fn from_repr(&self, val: Self::Repr) -> Value { json!(val) }
    fn to_repr(&self, val: Value) -> Result<Self::Repr, Value> {
        val.as_i64().ok_or(val)
    }
}


pub struct Blob(pub usize, pub usize);

impl TypeDesc for Blob {
    type Repr = Vec<u8>;
    fn as_json(&self) -> Value { json!(["blob", self.0, self.1]) }
    fn from_repr(&self, val: Self::Repr) -> Value { unimplemented!() }
    fn to_repr(&self, val: Value) -> Result<Self::Repr, Value> { unimplemented!() }
}


pub struct String;

impl TypeDesc for String {
    type Repr = StdString;
    fn as_json(&self) -> Value { json!(["string"]) }
    fn from_repr(&self, val: Self::Repr) -> Value { json!(val) }
    fn to_repr(&self, val: Value) -> Result<Self::Repr, Value> {
        val.as_str().map(Into::into).ok_or(val)
    }
}


pub struct StringUpto(pub usize);

impl TypeDesc for StringUpto {
    type Repr = StdString;
    fn as_json(&self) -> Value { json!(["string", 0, self.0]) }
    fn from_repr(&self, val: Self::Repr) -> Value { json!(val) }
    fn to_repr(&self, val: Value) -> Result<Self::Repr, Value> {
        val.as_str().map(Into::into).ok_or(val)
    }
}


pub struct Enum(pub HashMap<StdString, i64>);

impl TypeDesc for Enum {
    type Repr = i64;
    fn as_json(&self) -> Value {
        json!(["enum", self.0])
    }
    fn from_repr(&self, val: Self::Repr) -> Value { json!(val) }
    fn to_repr(&self, val: Value) -> Result<Self::Repr, Value> {
        if let Some(s) = val.as_str() {
            self.0.get(s).cloned().ok_or(val)
        } else if let Some(i) = val.as_i64() {
            if self.0.values().any(|&j| i == j) { Ok(i) }
            else { Err(val) }
        } else {
            Err(val)
        }
    }
}


pub struct ArrayOf<T: TypeDesc>(pub T);

impl<T: TypeDesc> TypeDesc for ArrayOf<T> {
    type Repr = Vec<T::Repr>;
    fn as_json(&self) -> Value {
        json!(["array", self.0.as_json()])
    }
    fn from_repr(&self, val: Self::Repr) -> Value { unimplemented!() }
    fn to_repr(&self, val: Value) -> Result<Self::Repr, Value> { unimplemented!() }
}


pub struct ArrayOfUpto<T: TypeDesc>(pub T, pub usize);

impl<T: TypeDesc> TypeDesc for ArrayOfUpto<T> {
    type Repr = Vec<T::Repr>;
    fn as_json(&self) -> Value {
        json!(["array", self.0.as_json(), self.1])
    }
    fn from_repr(&self, val: Self::Repr) -> Value { unimplemented!() }
    fn to_repr(&self, val: Value) -> Result<Self::Repr, Value> { unimplemented!() }
}


/*

TODO

pub struct TupleOf(Vec<Box<dyn TypeDesc>>);

impl TypeDesc for TupleOf {
    fn as_json(&self) -> Value {
        let subtypes = self.0.iter().map(|t| t.as_json()).collect::<Vec<_>>();
        json!(["tuple", subtypes])
    }
}


pub struct StructOf(HashMap<StdString, Box<dyn TypeDesc>>);

impl TypeDesc for StructOf {
    fn as_json(&self) -> Value {
        let subtypes = self.0.iter().map(|(k, v)| (k, v.as_json()))
                                    .collect::<HashMap<_, _>>();
        json!(["struct", subtypes])
    }
}
*/

pub struct Command<A: TypeDesc, R: TypeDesc>(pub A, pub R);

impl<A: TypeDesc, R: TypeDesc> TypeDesc for Command<A, R> {
    type Repr = (A::Repr, R::Repr);
    fn as_json(&self) -> Value { json!(["command", self.0.as_json(), self.1.as_json()]) }
    fn from_repr(&self, val: Self::Repr) -> Value { unimplemented!() }
    fn to_repr(&self, val: Value) -> Result<Self::Repr, Value> { unimplemented!() }
}


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



// Descriptive Data hierarchy


// pub type Str<'a> = Cow<'a, str>;

// #[derive(Serialize)]
// pub struct NodeDesc<'a> {
//     modules: Vec<(Str<'a>, ModuleDesc<'a>)>,
//     equipment_id: Str<'a>,
//     firmware: Str<'a>,
//     version: Str<'a>,
// }

// #[derive(Serialize)]
// pub struct ModuleDesc<'a> {
//     accessibles: Vec<(String, AccessibleDesc<'a>)>,
//     properties: HashMap<Str<'a>, Str<'a>>,
// }

// #[derive(Serialize)]
// pub struct AccessibleDesc<'a> {
//     description: Str<'a>,
//     datatype: TypeDesc,
//     #[serde(skip_serializing_if = "Option::is_none")]
//     readonly: Option<bool>,
//     #[serde(skip_serializing_if = "Option::is_none")]
//     unit: Option<Str<'a>>,
//     #[serde(skip_serializing_if = "Option::is_none")]
//     group: Option<Str<'a>>,
// }

// This is a mess :(
//
// In order to generate the param/cmd types as statics, we need not only
// the value given by the user, e.g. `DoubleFrom(0.)`, but also its type
// (since statics don't infer the type).
//
// This provides a working but messy way of doing this, for now.
#[macro_export]
macro_rules! datatype_type {
    (None) => (None);
    (Bool) => (Bool);
    (Double) => (Double);
    (DoubleFrom($($_:tt)*)) => (DoubleFrom);
    (DoubleFromTo($($_:tt)*)) => (DoubleFromTo);
    (Integer) => (Integer);
    (IntegerFrom($($_:tt)*)) => (IntegerFrom);
    (IntegerFromTo($($_:tt)*)) => (IntegerFromTo);
    (Blob($($_:tt)*)) => (Blob);
    (String) => (String);
    (StringUpto($($_:tt)*)) => (StringUpto);
    (Enum $($_:tt)*) => (Enum);
    (ArrayOf($($tp:tt)*)) => (ArrayOf<datatype_type!($($tp)*)>);
    (ArrayOfUpto($($tp:tt)*, $($_:tt)*)) => (ArrayOfUpto<datatype_type!($($tp)*)>);
    (Command($($tp1:tt)*, $($tp2:tt)*)) => (Command<datatype_type!($($tp1)*),
                                                    datatype_type!($($tp2)*)>);
}
