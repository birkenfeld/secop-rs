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
//! * `TypeDesc` can be derived for enums and structs, and provides a type-
//!   safe way to declare parameters and commands with enum and struct
//!   datatypes.

#![recursion_limit="256"]

mod module;
mod typedesc;

use synstructure::decl_derive;

decl_derive!([ModuleBase, attributes(param, command)] => crate::module::derive_module);
decl_derive!([TypeDesc, attributes(datatype)] => crate::typedesc::derive_typedesc);
