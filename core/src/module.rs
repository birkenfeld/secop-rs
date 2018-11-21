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
use crossbeam_channel::{Sender, Receiver, tick, select};

use crate::config::ModuleConfig;
use crate::errors::Error;
use crate::proto::{IncomingMsg, Msg};
use crate::server::HId;

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
}

/// Part of the Module trait to be implemented by user.
pub trait Module : ModuleBase {
    fn create(internals: ModInternals) -> Self where Self: Sized;
}

/// Part of the Module trait to be implemented by the derive macro.
pub trait ModuleBase {
    type ParamCache: Default;

    fn describe(&self) -> Value;
    fn command(&mut self, cmd: &str, args: Value) -> Result<Value, Error>;
    fn read(&mut self, param: &str) -> Result<Value, Error>;
    fn change(&mut self, param: &str, value: Value) -> Result<Value, Error>;
    fn init_updates(&mut self) -> Vec<Msg>;
    // TODO: is a result necessary?
    fn init_params(&mut self) -> Result<(), Error>;

    fn poll_normal(&mut self, n: usize);
    fn poll_busy(&mut self, n: usize);

    #[inline]
    fn internals(&self) -> &ModInternals;
    #[inline]
    fn name(&self) -> &str { &self.internals().name }
    #[inline]
    fn config(&self) -> &ModuleConfig { &self.internals().config }
    #[inline]
    fn req_receiver(&self) -> &Receiver<(HId, IncomingMsg)> { &self.internals().req_receiver }
    #[inline]
    fn rep_sender(&self) -> &Sender<(Option<HId>, Msg)> { &self.internals().rep_sender }

    fn send_update(&self, param: &str, value: Value, tstamp: f64) {
        self.rep_sender().send((None,
                                Msg::Update { module: self.name().into(),
                                              param: param.into(),
                                              data: json!([value, {"t": tstamp}]) })).unwrap();
    }

    fn run(mut self) where Self: Sized {
        mlzlog::set_thread_prefix(format!("[{}] ", self.name()));

        self.rep_sender().send((None,
                                Msg::Describing { id: self.name().into(),
                                                  structure: self.describe() })).unwrap();

        // TODO: customizable poll interval
        let poll = tick(Duration::from_millis(1000));
        let poll_busy = tick(Duration::from_millis(200));

        let mut poll_normal_counter = 0usize;
        let mut poll_busy_counter = 0usize;

        if let Err(e) = self.init_params() {
            warn!("error initializing params: {}", e);
        }

        loop {
            select! {
                recv(self.req_receiver()) -> res => if let Ok((hid, req)) = res {
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
                                               updates: self.init_updates() }
                        },
                        _ => {
                            warn!("message should not arrive here: {}", req);
                            continue;
                        }
                    };
                    self.rep_sender().send((Some(hid), rep)).unwrap();
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
