//! Demo cryo module.

use std::sync::Arc;
use std::thread;
use std::time::Duration;
use parking_lot::Mutex;
use serde_json::Value;

use crate::errors::{Error, ErrorKind};
use crate::module::{Config, Module};
use crate::util::localtime;

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

fn clamp(v: f64, min: f64, max: f64) -> f64 { v.min(max).max(min) }
fn sleep(v: f64) { thread::sleep(Duration::from_float_secs(v)) }

/// Ported from the NICOS simulator, for comments see nicos/devices/generic/virtual.py
impl CryoSimulator {
    fn run(self) {
        const LOOPDELAY: f64 = 1.0;
        const MAXPOWER: f64 = 100.0;

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
                sleep(clamp(LOOPDELAY/damper - h, 0.1, 60.));
                continue;
            }

            let mut v = self.vars.lock();

            let heatflow = self.heat_link(v.regulation, v.sample);
            info!("sample = {:.5}, regulation = {:.5}, heatflow = {:.5}",
                   v.sample, v.regulation, heatflow);
            let newsample = (v.sample + h*(self.sample_leak(v.sample) -
                                           heatflow)/self.sample_cp(v.sample)).max(0.);
            let regdelta = v.heater * 0.01 * MAXPOWER + heatflow - self.cooler_power(v.regulation);
            let newregulation = (v.regulation + h*regdelta/self.cooler_cp(v.regulation)).max(0.);

            if v.control {
                if heatflow * lastflow != -100. {
                    if (newregulation - newsample) * (v.regulation - v.sample) < 0. {
                        damper += 1.;
                    }
                }
                lastflow = heatflow;

                let error = v.setpoint - newregulation;
                delta = (delta + v.regulation - newregulation) / 2.;
                let kp = v.k_p / 10.;
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
                    heat_v = (heat_v + v.heater) / 2.;
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
        let flow = (sampletemp - coolertemp) * (coolertemp + sampletemp).powi(2)/400.;
        let cp = clamp(self.cooler_cp(coolertemp) * self.sample_cp(sampletemp), 1., 10.);
        clamp(flow, -cp, cp)
    }

    fn sample_cp(&self, temp: f64) -> f64 {
        3. * (temp / 30.).atan() + 12. * temp / ((temp - 12.).powi(2) + 10.) + 0.5
    }

    fn sample_leak(&self, temp: f64) -> f64 {
        0.02/temp
    }
}

//#[derive(Module)]
pub struct Cryo {
    vars: Arc<Mutex<StateVars>>,
}

impl Module for Cryo {
    fn create(config: &Config) -> Self {
        let vars = StateVars { sample: 5.0, regulation: 3.0, control: true,
                               k_p: 50.0, k_i: 10.0, k_d: 0.0,
                               heater: 0.0, ramp: 0.0, ramping: false,
                               target: 0.0, setpoint: 0.0 };
        let vars = Arc::new(Mutex::new(vars));
        let sim = CryoSimulator { vars: Arc::clone(&vars) };
        thread::spawn(move || sim.run());
        Cryo { vars }
    }

    // NOTE: these manual implementations will be replaced by a derive macro
    // later, which will also provide validation and then call methods in the
    // form `change_target(value: f64) -> Result`.
    // The declaration might then be similar to the "parameters" class dict
    // in Python.

    fn get_api_description(&self) -> Value {
        // TODO
        Value::Null
    }
    fn change(&mut self, param: &str, value: Value) -> Result<Value, Error> {
        match param {
            "target" => if let Some(v) = value.as_f64() { self.vars.lock().target = v; },
            "ramp" => if let Some(v) = value.as_f64() { self.vars.lock().ramp = v; },
            "p" => if let Some(v) = value.as_f64() { self.vars.lock().k_p = v; },
            "i" => if let Some(v) = value.as_f64() { self.vars.lock().k_i = v; },
            "d" => if let Some(v) = value.as_f64() { self.vars.lock().k_d = v; },
            // TODO
            _ => return Err(Error::new(ErrorKind::NoSuchParameter))
        }
        Ok(json!([value, {}]))
    }
    fn command(&mut self, cmd: &str, args: Value) -> Result<Value, Error> {
        match cmd {
            "stop" => {
                let mut v = self.vars.lock();
                v.target = v.setpoint;
                Ok(Value::Null)
            }
            _ => Err(Error::new(ErrorKind::NoSuchCommand))
        }
    }
    fn trigger(&mut self, param: &str) -> Result<Value, Error> {
        match param {
            "value" => Ok(json!([self.vars.lock().regulation, {}])),
            "setpoint" => Ok(json!([self.vars.lock().setpoint, {}])),
            "status" => Ok({
                let is_ramping = self.vars.lock().ramping;
                if is_ramping { json!([[300, "ramping"], {}]) } else { json!([[100, ""], {}]) }
            }),
            "target" => Ok(json!([self.vars.lock().target, {}])),
            // TODO
            _ => return Err(Error::new(ErrorKind::NoSuchParameter))
        }
    }

}
