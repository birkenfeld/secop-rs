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
//! Module to communicate with a Toellner power supply via serial port.

use log::*;
use serde_json::json;

use secop_core::prelude::*;
use secop_derive::ModuleBase;


#[derive(ModuleBase)]
#[param(name="status", doc="status", datainfo="StatusType", readonly=true)]
#[param(name="value", doc="current value", datainfo="Double()", readonly=true)]
#[param(name="target", doc="target value", datainfo="Double()", readonly=false)]
#[param(name="iomod", doc="module name of port", datainfo="Str(maxchars=64)", readonly=true,
        mandatory=true, swonly=true, visibility="none")]
#[param(name="channel", doc="channel to control", datainfo="Int(min=1, max=2)", readonly=true,
        default="1", swonly=true, visibility="none")]
pub struct ToellnerPS {
    internals: ModInternals,
    params: ToellnerPSParams,
    io: Client,
}

impl Module for ToellnerPS {
    fn create(internals: ModInternals) -> Result<Self> {
        let params: ToellnerPSParams = Default::default();
        let iomod = internals.config().extract_param("iomod", &params.iomod.info)
            .ok_or_else(|| Error::config("invalid or missing iomod parameter"))?;
        Ok(ToellnerPS { internals, params,
                        io: Client::new(&iomod).map_err(
                            |e| e.amend(&format!(" (connecting to submodule {})", iomod)))? })
    }

    fn setup(&mut self) -> Result<()> {
        Ok(())
    }

    fn teardown(&mut self) {}
}

impl ToellnerPS {
    fn read_value(&mut self) -> Result<f64> {
        let reply = self.io.command("communicate", json!("MV1?"))?;
        reply[0].as_str().and_then(|v| v.parse().ok()).ok_or_else(
            || Error::comm_failed(format!("invalid comm reply: {}", reply[0])))
    }

    fn read_status(&mut self) -> Result<Status> {
        Ok((StatusConst::Idle, "idle".into()))
    }

    fn read_target(&mut self) -> Result<f64> {
        Ok(0.0)
    }

    fn write_target(&mut self, _tgt: f64) -> Result<f64> {
        Err(Error::bad_value("not implemented yet"))
    }
}
