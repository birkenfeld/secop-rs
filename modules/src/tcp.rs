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
use secop_derive::{ModuleBase, TypeDesc};

use crate::support::comm::{CommClient, CommThread};

#[derive(TypeDesc)]
struct CommElement {
    #[datatype="Str(1024)"]
    message: String,
    #[datatype="Double"]
    waittime: f64,
}


#[derive(ModuleBase)]
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
          argtype="ArrayOf(1, 16, CommElementType)",
          restype="ArrayOf(1, 16, Str(1024))")]
#[param(name="sol", doc="start-of-line", datatype="Str(8)", readonly=true,
        default="\"\"", swonly=true, visibility="none")]
#[param(name="eol", doc="end-of-line", datatype="Str(8)", readonly=true,
        default="\"\\n\"", swonly=true, visibility="none")]
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
        if self.cache.host.as_ref().is_empty() {
            return Err(Error::config("need a host configured"));
        }
        let address = format!("{}:{}", self.cache.host.as_ref(), self.cache.port.as_ref());
        let timeout = Duration::from_millis((self.cache.timeout.as_ref() * 1000.) as u64);

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
            self.cache.sol.as_ref().as_bytes(),
            self.cache.eol.as_ref().as_bytes(),
            timeout,
        )?);
        Ok(())
    }

    fn teardown(&mut self) {
        // close the connection and join the thread if it exists
        self.comm.take();
    }
}

struct SPError(serialport::Error);

impl From<SPError> for Error {
    fn from(e: SPError) -> Error {
        Error::comm_failed(std::error::Error::description(&e.0))
    }
}

impl TcpComm {
    fn convert(&self, bytes: Result<Vec<u8>>) -> Result<String> {
        bytes.map(|v| {
            String::from_utf8(v)
                .unwrap_or_else(|e| String::from_utf8_lossy(&e.into_bytes()).into_owned())
        })
    }

    fn get_comm(&self) -> Result<&CommClient<TcpStream>> {
        if let Some(ref comm) = self.comm {
            Ok(comm)
        } else {
            Err(Error::comm_failed("connection not open"))
        }
    }

    fn read_status(&mut self) -> Result<Status> {
        // TODO
        Ok((StatusConst::Idle, "idle".into()))
    }

    fn do_communicate(&self, arg: String) -> Result<String> {
        self.convert(self.get_comm()?.communicate(arg.as_bytes()))
    }

    fn do_writeline(&self, arg: String) -> Result<()> {
        self.get_comm()?.write(arg.as_bytes(), true).map(|_| ())
    }

    fn do_readline(&self, _: ()) -> Result<String> {
        self.convert(self.get_comm()?.readline())
    }

    fn do_write(&self, arg: String) -> Result<()> {
        self.get_comm()?.write(arg.as_bytes(), false).map(|_| ())
    }

    fn do_read(&self, _: ()) -> Result<String> {
        self.convert(self.get_comm()?.read(u32::max_value()))
    }

    fn do_multi_communicate(&self, _req: Vec<CommElement>) -> Result<Vec<String>> {
        panic!("multi_communicate is not yet implemented");
    }

    fn update_sol(&mut self, _: String) -> Result<()> { Ok(()) }
    fn update_eol(&mut self, _: String) -> Result<()> { Ok(()) }
    fn update_timeout(&mut self, _: f64) -> Result<()> { Ok(()) }
    fn update_host(&mut self, _: String) -> Result<()> { Ok(()) }
    fn update_port(&mut self, _: i64) -> Result<()> { Ok(()) }
}
