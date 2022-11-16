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
//! This module contains basic module functionality.

use std::fmt;
use std::ops::Deref;
use std::time::{Duration, Instant};
use log::*;
use serde_json::{Value, json};
use derive_new::new;
use mlzutil::time::localtime;
use crossbeam_channel::{tick, Receiver, select};

use crate::config::{ModuleConfig, Visibility};
use crate::errors::Error;
use crate::proto::Msg;
use crate::server::{ReqReceiver, ModRepSender};
use crate::types::TypeInfo;

/// Data that every module requires.
#[derive(new, Clone)]
pub struct ModInternals {
    name: String,
    config: ModuleConfig,
    req_receiver: ReqReceiver,
    rep_sender: ModRepSender,
    poll_tickers: (Receiver<Instant>, Receiver<Instant>),
}

impl ModInternals {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn config(&self) -> &ModuleConfig {
        &self.config
    }
    pub fn class(&self) -> &str {
        &self.config.class
    }
    pub fn req_receiver(&self) -> &ReqReceiver {
        &self.req_receiver
    }
}

/// Data bag for a single parameter value.
pub struct ModParam<I: TypeInfo> {
    data: I::Repr,
    time: f64,
    /// TypeInfo for the parameter
    pub info: I,
}

impl<I: TypeInfo> Deref for ModParam<I> {
    type Target = I::Repr;
    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<I: TypeInfo> fmt::Display for ModParam<I> where I::Repr: fmt::Display {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}", self.data)
    }
}

impl<I: TypeInfo> fmt::Debug for ModParam<I> where I::Repr: fmt::Debug {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{:?}", self.data)
    }
}

impl<I: TypeInfo> ModParam<I>
where I::Repr: PartialEq + Clone + Default
{
    pub fn new(info: I) -> Self {
        Self { data: Default::default(), time: localtime(), info }
    }

    pub fn set(&mut self, value: I::Repr) {
        self.time = localtime();
        self.data = value;
    }

    /// Gets a newly determined value for this parameter, which is then cached,
    /// possibly an update message is sent, and the value is returned JSONified
    /// for sending in a reply.
    pub fn update(&mut self, value: I::Repr) -> Result<(Value, f64, bool), Error> {
        self.time = localtime();
        let is_update = if value != self.data {
            self.data = value.clone();
            true
        } else {
            false
        };
        Ok((self.info.to_json(value)?, self.time, is_update))
    }

    pub fn time(&self) -> f64 {
        self.time
    }

    pub fn to_json(&self) -> Result<Value, Error> {
        self.info.to_json(self.data.clone())
    }
}

/// Part of the Module trait to be implemented by user.
pub trait Module : ModuleBase {
    fn create(internals: ModInternals) -> Result<Self, Error> where Self: Sized;
    fn setup(&mut self) -> Result<(), Error>;
    fn teardown(&mut self);
}

/// Part of the Module trait that is implemented by the derive macro.
pub trait ModuleBase {
    /// Return the descriptive data for this module (a JSON object).
    fn describe(&self) -> Value;
    /// Execute a command.
    fn command(&mut self, cmd: &str, args: Value) -> Result<Value, Error>;
    /// Read a parameter and possibly emit an update message.
    fn read(&mut self, param: &str) -> Result<Value, Error>;
    /// Change a parameter and possibly emit an update message.
    fn change(&mut self, param: &str, value: Value) -> Result<Value, Error>;
    // TODO: is a result necessary?
    /// Initialize cached values for all parameters.
    fn init_params(&mut self) -> Result<(), Error>;
    /// Get a list of updates for all parameters, which must be sent upon
    /// activation of the module.
    fn activate_updates(&mut self) -> Vec<Msg>;

    /// Poll parameters.  If device is busy, parameters that participate in
    /// busy-poll are not polled.
    fn poll_normal(&mut self, n: usize);
    /// Poll parameters that participate in busy-poll if device status is busy.
    fn poll_busy(&mut self, n: usize);

    /// Return a reference to the module internals.  Even though we require
    /// the internals to be a member with a fixed name, the member is not
    /// known in the `run` method below.
    fn internals(&self) -> &ModInternals;
    fn internals_mut(&mut self) -> &mut ModInternals;
    #[inline]
    fn name(&self) -> &str { &self.internals().name }
    #[inline]
    fn config(&self) -> &ModuleConfig { &self.internals().config }

    /// Determine and set the initial value for a parameter.
    ///
    /// This is quite complex since we have multiple sources (defaults from
    /// code, config file, hardware) and multiple ways of using them (depending
    /// on whether the parameter is writable at runtime).
    fn init_parameter<I>(
        &mut self, param: &str, cached: impl Fn(&mut Self) -> &mut ModParam<I>,
        update: impl Fn(&mut Self, I::Repr) -> Result<(), Error>,
        swonly: bool, readonly: bool, default: Option<impl Fn() -> I::Repr>
    ) -> Result<(), Error>
        where I: TypeInfo, I::Repr: Clone + PartialEq + Default
    {
        let datainfo = cached(self).info.clone();
        if swonly {
            let value = if let Some(def) = default {
                if let Some(val) = self.config().parameters.get(param) {
                    debug!("initializing value for param {} (from config)", param);
                    datainfo.from_json(val)?
                } else {
                    debug!("initializing value for param {} (from default)", param);
                    def().into()
                }
            } else {
                // must be mandatory
                debug!("initializing value for param {} (from config)", param);
                datainfo.from_json(&self.config().parameters[param])?
            };
            cached(self).set(value);
            if !readonly {
                let value = cached(self).clone();
                update(self, value)?;
            }
        } else {
            if !readonly {
                if let Some(def) = default {
                    let value = if let Some(val) = self.config().parameters.get(param) {
                        debug!("initializing value for param {} (from config)", param);
                        val.clone()
                    } else {
                        debug!("initializing value for param {} (from default)", param);
                        datainfo.to_json(def().into())?
                    };
                    // This will emit an update message, but since the server is starting
                    // up, we can assume it hasn't been activated yet.
                    self.change(param, value)?;
                } else {
                    if let Some(val) = self.config().parameters.get(param) {
                        debug!("initializing value for param {} (from config)", param);
                        let val = val.clone();
                        self.change(param, val)?;
                    } else {
                        debug!("initializing value for param {} (from hardware)", param);
                        self.read(param)?;
                    }
                }
            } else {
                debug!("initializing value for param {} (from hardware)", param);
                self.read(param)?;
            }
        }
        Ok(())
    }

    /// Send a general update message back to the dispatcher, which decides if
    /// and where to send it on.
    fn send_update(&self, param: &str, value: Value, tstamp: f64) {
        self.internals().rep_sender.send(
            (None, Msg::Update { module: self.name().into(),
                                 param: param.into(),
                                 data: json!([value, {"t": tstamp}]) })).unwrap();
    }

    /// Updates the regular poll interval to the given value in seconds, and the
    /// busy poll interval to 1/5 of it.
    ///
    /// This is like an ordinary `update_param` method, but on the trait since
    /// it is always implemented the same.
    fn update_pollinterval(&mut self, val: f64) -> Result<(), Error> {
        self.internals_mut().poll_tickers = (
            tick(Duration::from_millis((val * 1000.) as u64)),
            tick(Duration::from_millis((val * 200.) as u64)),
        );
        Ok(())
    }

    /// Runs the main loop for the module, which does the following:
    ///
    /// * Initialize the module parameters
    /// * Handle incoming requests
    /// * Poll parameters periodically
    fn run(mut self) where Self: Sized + Module {
        mlzlog::set_thread_prefix(format!("[{}] ", self.name()));

        // Do initialization steps.  On failure, we panic, which will be caught
        // upstream and retries are scheduled accordingly.
        if let Err(e) = self.init_params() {
            panic!("error initializing params: {}", e);
        }
        if let Err(e) = self.setup() {
            panic!("setup failed: {}", e);
        }

        // Tell the dispatcher how to describe ourselves.  If the visibility is
        // "none", the module is assumed to be internal-use only.
        if self.config().visibility != Visibility::None {
            self.internals().rep_sender.send(
                (None, Msg::Describing { id: self.name().into(),
                                         structure: self.describe() })).unwrap();
        }

        let mut poll_normal_counter = 0usize;
        let mut poll_busy_counter = 0usize;

        loop {
            select! {
                recv(self.internals().req_receiver) -> res => if let Ok((hid, req)) = res {
                    // These are the only messages that are handled here.  They all
                    // generate a reply, which is sent back to the dispatcher.
                    let rep = match req.1 {
                        Msg::Read { module, param } => match self.read(&param) {
                            Ok(data) => Msg::Update { module, param, data },
                            Err(e) => e.into_msg(req.0),
                        },
                        Msg::Change { module, param, value } => match self.change(&param, value) {
                            Ok(data) => Msg::Changed { module, param, data },
                            Err(e) => e.into_msg(req.0),
                        },
                        Msg::Do { module, command, arg } => match self.command(&command, arg) {
                            Ok(data) => Msg::Done { module, command, data },
                            Err(e) => e.into_msg(req.0),
                        },
                        Msg::Activate { module } => {
                            Msg::InitUpdates { module: module,
                                               updates: self.activate_updates() }
                        },
                        _ => {
                            warn!("message should not arrive here: {}", req);
                            continue;
                        }
                    };
                    self.internals().rep_sender.send((Some(hid), rep)).unwrap();
                },
                // TODO: decide if polling "atomically" (i.e. all parameters at once)
                // is ok, since it could delay client requests.
                recv(self.internals().poll_tickers.0) -> _ => {
                    self.poll_normal(poll_normal_counter);
                    poll_normal_counter = poll_normal_counter.wrapping_add(1);
                },
                recv(self.internals().poll_tickers.1) -> _ => {
                    self.poll_busy(poll_busy_counter);
                    poll_busy_counter = poll_busy_counter.wrapping_add(1);
                }
            }
        }
    }
}
