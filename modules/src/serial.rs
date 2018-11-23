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
use serialport::{self, SerialPort};

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
#[param(name="devfile", doc="device file name", datatype="Str(128)", readonly=true,
        mandatory=true, swonly=true, visibility="none")]
#[param(name="baudrate", doc="baud rate", datatype="Int(1200, 230400)", readonly=true,
        default="9600", swonly=true, visibility="none")]
pub struct SerialComm {
    internals: ModInternals,
    cache: SerialCommParamCache,
    comm: Option<CommClient<Box<SerialPort>>>,
}

impl Module for SerialComm {
    fn create(internals: ModInternals) -> Result<Self> {
        Ok(SerialComm { internals, cache: Default::default(), comm: None })
    }

    fn setup(&mut self) -> Result<()> {
        let devfile = self.cache.devfile.cloned();
        if devfile.is_empty() {
            return Err(Error::config("need a devfile configured"));
        }
        let timeout = Duration::from_millis((self.cache.timeout.as_ref() * 1000.) as u64);
        let baudrate = self.cache.baudrate.cloned() as u32;

        let connect = move || -> Result<(Box<SerialPort>, Box<SerialPort>)> {
            info!("opening {}...", devfile);
            let mut port = serialport::open(&devfile).map_err(SPError)?;
            let mut settings = port.settings();
            settings.baud_rate = baudrate;
            port.set_all(&settings).unwrap();
            let mut rport = serialport::open(&devfile).map_err(SPError)?;
            rport.set_all(&settings).unwrap();
            Ok((rport, port))
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

impl SerialComm {
    fn convert(&self, bytes: Result<Vec<u8>>) -> Result<String> {
        bytes.map(|v| {
            String::from_utf8(v)
                .unwrap_or_else(|e| String::from_utf8_lossy(&e.into_bytes()).into_owned())
        })
    }

    fn get_comm(&self) -> Result<&CommClient<Box<SerialPort>>> {
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
    fn update_devfile(&mut self, _: String) -> Result<()> { Ok(()) }
    fn update_baudrate(&mut self, _: i64) -> Result<()> { Ok(()) }
}