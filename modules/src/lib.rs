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

use std::panic::catch_unwind;
use std::error::Error as StdError;
use std::thread::Builder;
use log::*;

use secop_core::module::{Module, ModInternals};


/// Inner (generic) implementation of `run_module`.
fn inner_run<T: Module>(internals: ModInternals) {
    let name = internals.name().to_owned();
    Builder::new().name(name.clone()).spawn(move || loop {
        if catch_unwind(|| T::create(internals.clone()).run()).is_err() {
            error!("module {} panicked; restarting...", name);
        }
    }).expect("could not start thread");
}


/// Start the module's own thread.
pub fn run_module(internals: ModInternals) -> Result<(), Box<StdError>> {
    Ok(match &*internals.class() {
        "Cryo" => inner_run::<cryo::Cryo>(internals),
        _ => return Err(format!("no such module class: {}", internals.class()).into())
    })
}
