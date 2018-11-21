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
//   Enrico Faulhaber <enrico.faulhaber@frm2.tum.de>
//   Georg Brandl <g.brandl@fz-juelich.de>
//
// -----------------------------------------------------------------------------
//
//! Demo cryo module.

use std::sync::Arc;
use std::thread;
use std::time::Duration;
use log::*;
use parking_lot::Mutex;
use mlzutil::time::localtime;

// These should later be put into a "core" or "prelude" type export module.
use secop_core::errors::Result;
use secop_core::module::{Module, ModuleBase, ModInternals};
use secop_core::types::*;
use secop_derive::{ModuleBase, TypeDesc};

#[derive(Default)]
struct StateVars {
    // updated by user:
    target: f64,
    ramp: f64,
    control: bool,
    k_p: f64,
    k_i: f64,
    k_d: f64,

    // updated by simulator:
    setpoint: f64,
    ramping: bool,
    regulation: f64,
    sample: f64,
    heater: f64,
}


struct CryoSimulator {
    vars: Arc<Mutex<StateVars>>,
}

fn clamp(v: f64, min: f64, max: f64) -> f64 { v.min(max.max(min)).max(min.min(max)) }
fn sleep_ms(v: u64) { thread::sleep(Duration::from_millis(v)) }

/// Ported from the NICOS simulator, for comments see nicos/devices/generic/virtual.py
impl CryoSimulator {
    fn run(self) {
        const LOOPDELAY: f64 = 1.0;
        const MAXPOWER: f64 = 10.0;

        mlzlog::set_thread_prefix("[CryoSim] ".into());

        let mut last_t = localtime();
        let mut last_control = false;
        let mut damper = 1.0;
        let mut lastflow = 0.0;
        let mut delta = 0.0;
        let mut iaccum = 0.0;
        let mut last_dtot = 0.0;
        let mut last_heaters = (0.0, 0.0);

        loop {
            let t = localtime();
            let h = t - last_t;

            if h < LOOPDELAY/damper {
                sleep_ms((1000. * clamp(LOOPDELAY/damper - h, 0.1, 60.)) as u64);
                continue;
            }

            let mut v = self.vars.lock();

            let heatflow = self.heat_link(v.regulation, v.sample);
            let newsample = (v.sample + h*(self.sample_leak(v.sample) -
                                           heatflow)/self.sample_cp(v.sample)).max(0.);
            let newsample = clamp(newsample, v.sample, v.regulation);
            let regdelta = v.heater * 0.01 * MAXPOWER + heatflow -
                self.cooler_power(v.regulation);
            let newregulation = (v.regulation +
                                 h*regdelta/self.cooler_cp(v.regulation)).max(0.);
            info!("sample = {:.5}, regulation = {:.5}, heatflow = {:.5}",
                   newsample, newregulation, heatflow);

            if v.control {
                if heatflow * lastflow != -100. {
                    if (newregulation - newsample) * (v.regulation - v.sample) < 0. {
                        damper += 1.;
                    }
                }
                lastflow = heatflow;

                let error = v.setpoint - newregulation;
                delta = (delta + v.regulation - newregulation) * 0.5;
                let kp = v.k_p * 0.1;
                let ki = kp * v.k_i.abs() / 500.;
                let kd = kp * v.k_d.abs() / 2.;

                let ptot = kp * error;
                iaccum += ki * error * h;
                let dtot = kd * delta / h;

                iaccum = clamp(iaccum, 0., 100.);

                if last_control != v.control {
                    iaccum = v.heater - ptot - dtot;
                }

                let mut heat_v = ptot + iaccum + dtot;
                if damper > 1.0 {
                    heat_v = ((damper.powi(2) - 1.)*v.heater + heat_v) / damper.powi(2);
                }

                if dtot * last_dtot < -0.2 {
                    heat_v = (heat_v + v.heater) * 0.5;
                }
                v.heater = clamp(heat_v, 0., 100.);
                last_dtot = dtot;

                info!("PID: P = {:.2}, I = {:.2}, D = {:.2}, heater = {:.2}",
                       ptot, iaccum, dtot, v.heater);

                let (x, y) = last_heaters;
                if (x + 0.1 < y && y > v.heater + 0.1) || (x > y + 0.1 && y + 0.1 < v.heater) {
                    damper += 1.;
                }
                last_heaters = (y, v.heater);
            } else {
                last_heaters = (0., 0.);
            }

            v.sample = newsample;
            v.regulation = newregulation;
            last_control = v.control;

            if v.setpoint != v.target {
                let maxdelta = if v.ramp == 0. {
                    10000.
                } else {
                    v.ramp / 60. * h
                };
                v.setpoint += clamp(v.target - v.setpoint, -maxdelta, maxdelta);
                info!("setpoint changes to {:.3} (target {:.3})", v.setpoint, v.target);
            }

            if v.setpoint == v.target {
                v.ramping = false;
                damper -= (damper - 1.)/10.;
            } else {
                v.ramping = true;
            }
            damper -= (damper - 1.)/20.;

            last_t = t;
        }
    }

    /// returns cooling power in W at given temperature
    fn cooler_power(&self, temp: f64) -> f64 {
        // quadratic up to 42K, is linear from 40W@42K to 100W@600K
        clamp(15. * (temp * 0.01).atan().powi(3), 0., 40.) + temp * 0.1 - 0.2
    }

    /// heat capacity of cooler at given temp
    fn cooler_cp(&self, temp: f64) -> f64 {
        return 75. * (temp / 50.).atan().powi(2) + 1.
    }

    /// heatflow from sample to cooler. may be negative...
    fn heat_link(&self, coolertemp: f64, sampletemp: f64) -> f64 {
        let flow = (sampletemp - coolertemp) * (coolertemp + sampletemp).powi(2) / 400.;
        let cp = clamp(self.cooler_cp(coolertemp) * self.sample_cp(sampletemp), 1., 10.);
        clamp(flow, -cp, cp)
    }

    fn sample_cp(&self, temp: f64) -> f64 {
        3.*(temp / 30.).atan() + 12.*temp / ((temp - 12.).powi(2) + 10.) + 0.5
    }

    fn sample_leak(&self, temp: f64) -> f64 {
        0.02 / temp
    }
}

#[derive(TypeDesc, Clone, PartialEq)]
enum Mode {
    PID,
    OpenLoop,
}

impl Default for Mode {
    fn default() -> Self { Mode::PID }
}

#[derive(TypeDesc, Clone, PartialEq, Default)]
struct PID {
    #[datatype="DoubleFrom(0.0)"]
    p: f64,
    #[datatype="DoubleFrom(0.0)"]
    i: f64,
    #[datatype="DoubleFrom(0.0)"]
    d: f64,
}

#[derive(ModuleBase)]
#[param(name="status", doc="status",
        datatype="StatusType",
        readonly=true)]
#[param(name="value", doc="regulation temperature",
        datatype="DoubleFrom(0.0)",
        readonly=true, unit="K")]
#[param(name="sample", doc="sample temperature",
        datatype="DoubleFrom(0.0)",
        readonly=true, unit="K")]
#[param(name="target", doc="target temperature",
        datatype="DoubleFrom(0.0)",
        readonly=false, default="0.0", unit="K")]
#[param(name="setpoint", doc="current setpoint for the temperature",
        datatype="DoubleFrom(0.0)",
        readonly=true, unit="K")]
#[param(name="ramp", doc="setpoint ramping speed",
        datatype="DoubleRange(0.0, 1e3)",
        readonly=false, default="1.0", unit="K/min")]
#[param(name="heater", doc="current heater setting",
        datatype="DoubleRange(0.0, 100.0)",
        readonly=true, unit="%")]
#[param(name="p", doc="regulation coefficient P",
        datatype="DoubleFrom(0.0)", polling="-5",
        readonly=false, default="40.0", unit="%/K", group="pid")]
#[param(name="i", doc="regulation coefficient I",
        datatype="DoubleRange(0.0, 100.0)", polling="-5",
        readonly=false, default="10.0", group="pid")]
#[param(name="d", doc="regulation coefficient D",
        datatype="DoubleRange(0.0, 100.0)", polling="-5",
        readonly=false, default="2.0", group="pid")]
#[param(name="pid", doc="regulation coefficients",
        datatype="PIDType", polling="0",
        readonly=false, group="pid")]
#[param(name="mode", doc="regulation mode",
        datatype="ModeType", polling="0",
        readonly=false, default="Mode::PID", group="pid")]
#[command(name="stop", doc="stop ramping the setpoint",
          argtype="None", restype="None")]
pub struct Cryo {
    internals: ModInternals,
    pcache: CryoParamCache,
    vars: Arc<Mutex<StateVars>>,
}

impl Module for Cryo {
    fn create(internals: ModInternals) -> Self {
        let vars = StateVars { sample: 5.0, regulation: 3.0, control: true,
                               k_p: 40.0, k_i: 10.0, k_d: 2.0,
                               heater: 0.0, ramp: 0.0, ramping: false,
                               target: 0.0, setpoint: 0.0 };
        let vars = Arc::new(Mutex::new(vars));
        let sim = CryoSimulator { vars: Arc::clone(&vars) };
        let pcache = CryoParamCache::default();
        thread::spawn(move || sim.run());
        Cryo { internals, pcache, vars }
    }

}

impl Cryo {
    fn read_value(&mut self)    -> Result<f64> { Ok(self.vars.lock().regulation) }
    fn read_sample(&mut self)   -> Result<f64> { Ok(self.vars.lock().sample) }
    fn read_target(&mut self)   -> Result<f64> { Ok(self.vars.lock().target) }
    fn read_setpoint(&mut self) -> Result<f64> { Ok(self.vars.lock().setpoint) }
    fn read_ramp(&mut self)     -> Result<f64> { Ok(self.vars.lock().ramp) }
    fn read_heater(&mut self)   -> Result<f64> { Ok(self.vars.lock().heater) }
    fn read_p(&mut self)        -> Result<f64> { Ok(self.vars.lock().k_p) }
    fn read_i(&mut self)        -> Result<f64> { Ok(self.vars.lock().k_i) }
    fn read_d(&mut self)        -> Result<f64> { Ok(self.vars.lock().k_d) }
    fn read_pid(&mut self)      -> Result<PID> {
        let v = self.vars.lock();
        Ok(PID { p: v.k_p, i: v.k_i, d: v.k_d })
    }
    fn read_mode(&mut self)     -> Result<Mode> {
        Ok(if self.vars.lock().control { Mode::PID } else { Mode::OpenLoop })
    }
    fn read_status(&mut self)   -> Result<Status> {
        Ok(if self.vars.lock().ramping {
            (StatusConst::Busy, "ramping".into())
        } else {
            (StatusConst::Idle, "idle".into())
        })
    }

    fn write_target(&mut self, value: f64) -> Result<()> { Ok(self.vars.lock().target = value) }
    fn write_ramp(&mut self, value: f64)   -> Result<()> { Ok(self.vars.lock().ramp = value) }
    fn write_p(&mut self, value: f64)      -> Result<()> {
        self.vars.lock().k_p = value;
        let _ = self.read("pid");
        Ok(())
    }
    fn write_i(&mut self, value: f64)      -> Result<()> {
        self.vars.lock().k_i = value;
        let _ = self.read("pid");
        Ok(())
    }
    fn write_d(&mut self, value: f64)      -> Result<()> {
        self.vars.lock().k_d = value;
        let _ = self.read("pid");
        Ok(())
    }
    fn write_mode(&mut self, value: Mode)  -> Result<()> {
        Ok(self.vars.lock().control = value == Mode::PID)
    }

    fn write_pid(&mut self, value: PID) -> Result<()> {
        {
            let mut v = self.vars.lock();
            v.k_p = value.p;
            v.k_i = value.i;
            v.k_d = value.d;
        }
        let _ = self.read("p");
        let _ = self.read("i");
        let _ = self.read("d");
        Ok(())
    }

    fn do_stop(&self, _: ()) -> Result<()> {
        let mut v = self.vars.lock();
        v.target = v.setpoint;
        Ok(())
    }
}
