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
//! The main entry point and crate definitions.

pub mod types;
pub mod proto;
pub mod server;
pub mod client;
pub mod config;
pub mod module;
pub mod errors;

// Hack to allow the derives to derive stuff in this crate.
// Does not need to be public for that.
mod secop_core {
    pub use crate::errors;
    pub use crate::types;
}

/// Re-exports mostly everything needed for writing modules.
pub mod prelude {
    pub use crate::errors::{Error, ErrorKind, Result};
    pub use crate::module::{ModInternals, ModuleBase, Module};
    pub use crate::config::{ServerConfig, ModuleConfig};
    pub use crate::client::Client;
    pub use crate::types::{TypeDesc, Null, Bool, Double, DoubleFrom,
                           DoubleRange, Int, Blob, Str, ArrayOf,
                           Tuple2, Tuple3, Tuple4, Tuple5, Tuple6,
                           Enum, StatusConst, StatusType, Status};
}
