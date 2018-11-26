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
//! Module to communicate via a TCP connection.

use std::net::TcpStream;
use std::time::Duration;
use log::*;

// These should later be put into a "core" or "prelude" type export module.
use secop_core::errors::{Error, Result};
use secop_core::module::{Module, ModInternals};
use secop_core::types::*;
use secop_derive::ModuleBase;

use crate::support::comm::{CommClient, CommThread, HasComm};


#[derive(ModuleBase)]
// TODO: factor out these common params/commands
#[param(name="status", doc="status", datatype="StatusType", readonly=true)]
#[command(name="communicate", doc="communicate (write/read cycle)",
          argtype="Str(1024)", restype="Str(1024)")]
#[command(name="readline", doc="read a message",
          argtype="Null", restype="Str(1024)")]
#[command(name="writeline", doc="write a message",
          argtype="Str(1024)", restype="Null")]
#[command(name="read", doc="read input buffer",
          argtype="Null", restype="Str(1024)")]
#[command(name="write", doc="write raw string",
          argtype="Str(1024)", restype="Null")]
#[command(name="multi_communicate", doc="do multiple communicate cycles",
          argtype="ArrayOf(1, 16, Tuple2(Str(1024), Double))",
          restype="ArrayOf(1, 16, Str(1024))")]
#[param(name="sol", doc="start-of-line", datatype="Str(8)", readonly=true,
        default="\"\".into()", swonly=true, visibility="none")]
#[param(name="eol", doc="end-of-line", datatype="Str(8)", readonly=true,
        default="\"\\n\".into()", swonly=true, visibility="none")]
#[param(name="timeout", doc="comm timeout", datatype="DoubleFrom(0.)", readonly=true,
        default="2.0", swonly=true, visibility="none")]
#[param(name="host", doc="host to connect to", datatype="Str(1024)", readonly=true,
        mandatory=true, swonly=true, visibility="none")]
#[param(name="port", doc="port to connect to", datatype="Int(1, 65535)", readonly=true,
        mandatory=true, swonly=true, visibility="none")]
pub struct TcpComm {
    internals: ModInternals,
    cache: TcpCommParamCache,
    comm: Option<CommClient<TcpStream>>,
}

impl Module for TcpComm {
    fn create(internals: ModInternals) -> Result<Self> {
        Ok(TcpComm { internals, cache: Default::default(), comm: None })
    }

    fn setup(&mut self) -> Result<()> {
        if self.cache.host.is_empty() {
            return Err(Error::config("need a host configured"));
        }
        let address = format!("{}:{}", self.cache.host, self.cache.port);
        let timeout = Duration::from_millis((*self.cache.timeout * 1000.) as u64);

        let connect = move || -> Result<(TcpStream, TcpStream)> {
            info!("connecting to {}...", address);
            let wstream = TcpStream::connect(address.as_str())?;
            wstream.set_write_timeout(Some(timeout))?;
            wstream.set_nodelay(true)?;
            let rstream = wstream.try_clone()?;
            info!("connection established to {}", address);
            Ok((rstream, wstream))
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

impl HasComm for TcpComm {
    type IO = TcpStream;

    fn get_comm(&self) -> Result<&CommClient<Self::IO>> {
        self.comm.as_ref().ok_or_else(|| Error::comm_failed("connection not open"))
    }
}
