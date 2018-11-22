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

use std::time::Duration;
use log::*;
use serde_json::{Value, json};
use derive_new::new;
use mlzutil::time::localtime;
use crossbeam_channel::{Sender, Receiver, tick, select};

use crate::config::{ModuleConfig, Visibility};
use crate::errors::Error;
use crate::proto::{IncomingMsg, Msg};
use crate::server::HId;
use crate::types::TypeDesc;

/// Data that every module requires.
#[derive(new, Clone)]
pub struct ModInternals {
    name: String,
    config: ModuleConfig,
    req_receiver: Receiver<(HId, IncomingMsg)>,
    rep_sender: Sender<(Option<HId>, Msg)>,
}

impl ModInternals {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn class(&self) -> &str {
        &self.config.class
    }
    pub fn req_receiver(&self) -> &Receiver<(HId, IncomingMsg)> {
        &self.req_receiver
    }
}

/// Cache for a single parameter value.
#[derive(Default)]
pub struct CachedParam<T> {
    data: T,
    time: f64,
}

impl<T: PartialEq + Clone> CachedParam<T> {
    pub fn new(value: T) -> Self {
        Self { data: value, time: localtime() }
    }

    /// Gets a newly determined value for this parameter, which is then cached,
    /// possibly an update message is sent, and the value is returned JSONified
    /// for sending in a reply.
    pub fn update<TD: TypeDesc<Repr=T>>(&mut self, value: T, td: &TD) -> Result<(Value, f64, bool), Error> {
        self.time = localtime();
        let is_update = if value != self.data {
            self.data = value.clone();
            true
        } else {
            false
        };
        Ok((td.to_json(value)?, self.time, is_update))
    }

    pub fn as_ref(&self) -> &T {
        &self.data
    }

    pub fn cloned(&self) -> T {
        self.data.clone()
    }

    pub fn time(&self) -> f64 {
        self.time
    }
}

/// Part of the Module trait to be implemented by user.
pub trait Module : ModuleBase {
    fn create(internals: ModInternals) -> Result<Self, Error> where Self: Sized;
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
    #[inline]
    fn internals(&self) -> &ModInternals;
    #[inline]
    fn name(&self) -> &str { &self.internals().name }
    #[inline]
    fn config(&self) -> &ModuleConfig { &self.internals().config }

    /// Send a general update message back to the dispatcher, which decides if
    /// and where to send it on.
    fn send_update(&self, param: &str, value: Value, tstamp: f64) {
        self.internals().rep_sender.send(
            (None, Msg::Update { module: self.name().into(),
                                 param: param.into(),
                                 data: json!([value, {"t": tstamp}]) })).unwrap();
    }

    /// Runs the main loop for the module, which does the following:
    ///
    /// * Initialize the module parameters
    /// * Handle incoming requests
    /// * Poll parameters periodically
    fn run(mut self) where Self: Sized {
        mlzlog::set_thread_prefix(format!("[{}] ", self.name()));

        // Tell the dispatcher how to describe ourselves.  If the visibility is
        // "none", the module is assumed to be internal-use only.
        if self.config().visibility != Visibility::None {
            self.internals().rep_sender.send(
                (None, Msg::Describing { id: self.name().into(),
                                         structure: self.describe() })).unwrap();
        }

        if let Err(e) = self.init_params() {
            warn!("error initializing params: {}", e);
            // TODO: and now?
        }

        // TODO: customizable poll interval
        let poll = tick(Duration::from_millis(1000));
        let poll_busy = tick(Duration::from_millis(200));

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
                recv(poll) -> _ => {
                    self.poll_normal(poll_normal_counter);
                    poll_normal_counter = poll_normal_counter.wrapping_add(1);
                },
                recv(poll_busy) -> _ => {
                    self.poll_busy(poll_busy_counter);
                    poll_busy_counter = poll_busy_counter.wrapping_add(1);
                }
            }
        }
    }
}
