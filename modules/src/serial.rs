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

use std::time::Duration;
use log::*;
use serialport::{self, TTYPort};

use secop_core::prelude::*;
use secop_derive::ModuleBase;

use crate::support::comm::{CommClient, CommThread, HasComm};


#[derive(ModuleBase)]
#[param(name="status", doc="status", datainfo="StatusType", readonly=true)]
#[command(name="communicate", doc="communicate (write/read cycle)",
          argtype="Str(maxchars=1024)", restype="Str(maxchars=1024)")]
#[command(name="readline", doc="read a message",
          argtype="Null", restype="Str(maxchars=1024)")]
#[command(name="writeline", doc="write a message",
          argtype="Str(maxchars=1024)", restype="Null")]
#[command(name="read", doc="read input buffer",
          argtype="Null", restype="Str(maxchars=1024)")]
#[command(name="write", doc="write raw string",
          argtype="Str(maxchars=1024)", restype="Null")]
#[command(name="multi_communicate", doc="do multiple communicate cycles",
          argtype="[(Str(maxchars=1024), Double()); 1..=16]",
          restype="[Str(maxchars=1024); 1..=16]")]
#[param(name="sol", doc="start-of-line", datainfo="Str(maxchars=8)", readonly=true,
        default="\"\".into()", swonly=true, visibility="none")]
#[param(name="eol", doc="end-of-line", datainfo="Str(maxchars=8)", readonly=true,
        default="\"\\n\".into()", swonly=true, visibility="none")]
#[param(name="timeout", doc="comm timeout", datainfo="Double(min=0.)", readonly=true,
        default="2.0", swonly=true, visibility="none")]
#[param(name="devfile", doc="device file name", datainfo="Str(maxchars=128)", readonly=true,
        mandatory=true, swonly=true, visibility="none")]
#[param(name="baudrate", doc="baud rate", datainfo="Int(min=1200, max=230400)", readonly=true,
        default="9600", swonly=true, visibility="none")]
pub struct SerialComm {
    internals: ModInternals,
    cache: SerialCommParamCache,
    comm: Option<CommClient<TTYPort>>,
}

impl Module for SerialComm {
    fn create(internals: ModInternals) -> Result<Self> {
        Ok(SerialComm { internals, cache: Default::default(), comm: None })
    }

    fn setup(&mut self) -> Result<()> {
        let devfile = self.cache.devfile.clone();
        if devfile.is_empty() {
            return Err(Error::config("need a devfile configured"));
        }
        let timeout = Duration::from_millis((*self.cache.timeout * 1000.) as u64);
        let baudrate = *self.cache.baudrate as u32;

        let connect = move || -> Result<(TTYPort, TTYPort)> {
            info!("opening {}...", devfile);
            let port = serialport::new(&devfile, baudrate)
                // no intrinsic timeout
                .timeout(Duration::from_secs(1_000_000_000))
                .open_native()
                .map_err(|e| Error::comm_failed(e.to_string()))?;
            let rport = port.try_clone_native().map_err(|e| Error::comm_failed(e.to_string()))?;
            Ok((rport, port))
        };

        self.comm = Some(CommThread::spawn(
            Box::new(connect),
            self.cache.sol.as_bytes(),
            self.cache.eol.as_bytes(),
            timeout,
        )?);
        Ok(())
    }

    fn teardown(&mut self) {
        // close the connection and join the thread if it exists
        self.comm.take();
    }
}

impl HasComm for SerialComm {
    type IO = TTYPort;

    fn get_comm(&self) -> Result<&CommClient<Self::IO>> {
        self.comm.as_ref().ok_or_else(|| Error::comm_failed("connection not open"))
    }
}
