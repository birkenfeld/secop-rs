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
//! Module dispatcher.

#[macro_use]
extern crate secop_core;

pub mod cryo;

use std::error::Error as StdError;
use std::thread;
use serde_json::Value;

use secop_core::module::{Module, ModInternals};


fn inner_run<T: Module + Send + 'static>(internals: ModInternals) -> Value {
    let module = T::create(internals);
    let descriptive = module.describe();
    // TODO: catch panics and restart
    thread::spawn(|| module.run());
    descriptive
}


pub fn run_module(internals: ModInternals) -> Result<Value, Box<StdError>> {
    Ok(match &*internals.class() {
        "Cryo" => inner_run::<cryo::Cryo>(internals),
        // TODO: return a proper error
        _ => panic!("No such module class: {}", internals.class())
    })
}
