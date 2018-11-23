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
//! Module to communicate via a serial port.

// use log::*;

// These should later be put into a "core" or "prelude" type export module.
use secop_core::errors::Result;
use secop_core::module::{Module, ModInternals};
use secop_core::types::*;
use secop_derive::{ModuleBase};

#[derive(ModuleBase)]
#[param(name="status", doc="status", datatype="StatusType", readonly=true)]
#[command(name="communicate", doc="communicate (write/read cycle)",
          argtype="Str(1024)", restype="Str(1024)")]
// #[command(name="read", doc="read a message",
//           argtype="Null", restype="Str(1024)")]
pub struct SerialComm {
    internals: ModInternals,
    cache: SerialCommParamCache,
}

impl Module for SerialComm {
    fn create(internals: ModInternals) -> Result<Self> {
        let cache = SerialCommParamCache::default();
        Ok(SerialComm { internals, cache })
    }

    fn teardown(&mut self) { }
}

impl SerialComm {
    fn read_status(&mut self) -> Result<Status> {
        Ok((StatusConst::Idle, "idle".into()))
    }

    fn do_communicate(&self, _req: String) -> Result<String> {
        Ok("".into())
    }
}
