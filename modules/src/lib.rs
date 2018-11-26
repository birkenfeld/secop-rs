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

#![feature(try_blocks)]

#[macro_use]
extern crate secop_core;

mod simcryo;
mod serial;
mod tcp;
mod toellner;

pub(crate) mod support;

use std::panic::catch_unwind;
use std::error::Error as StdError;
use std::time::Duration;
use std::thread::{Builder, sleep};
use log::*;

use secop_core::module::{Module, ModInternals};


/// Inner (generic) implementation of `run_module`.
fn inner_run<T: Module>(internals: ModInternals) {
    let name = internals.name().to_owned();
    Builder::new().name(name.clone()).spawn(move || loop {
        if catch_unwind(|| {
            T::create(internals.clone()).expect("init failed").run()
        }).is_err() {
            error!("module {} panicked, waiting...", name);
            // remove all pending requests
            internals.req_receiver().try_iter().count();
            // wait for another request to arrive
            while internals.req_receiver().is_empty() {
                sleep(Duration::from_millis(100));
            }
            info!("now restarting module {}", name);
        }
    }).expect("could not start thread");
}


/// Start the module's own thread.
pub fn run_module(internals: ModInternals) -> Result<(), Box<StdError>> {
    Ok(match &*internals.class() {
        "SimCryo" => inner_run::<simcryo::SimCryo>(internals),
        "SerialComm" => inner_run::<serial::SerialComm>(internals),
        "TcpComm" => inner_run::<tcp::TcpComm>(internals),
        "ToellnerPS" => inner_run::<toellner::ToellnerPS>(internals),
        _ => return Err(format!("no such module class: {}", internals.class()).into())
    })
}
