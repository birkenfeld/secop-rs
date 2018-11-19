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
use crate::proto::Msg;
use crate::server::HId;

/// Data that every module requires.
#[derive(new)]
pub struct ModInternals {
    name: String,
    config: ModuleConfig,
    req_receiver: Receiver<(HId, Msg)>,
    rep_sender: Sender<(Option<HId>, Msg)>,
}

impl ModInternals {
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
    fn change(&mut self, param: &str, value: Value) -> Result<Value, Error>;
    fn command(&mut self, cmd: &str, args: Value) -> Result<Value, Error>;
    fn trigger(&mut self, param: &str) -> Result<Value, Error>;
    fn describe(&self) -> Value;

    fn poll_params(&self) -> &'static [&'static str];

    #[inline]
    fn internals(&self) -> &ModInternals;
    #[inline]
    fn name(&self) -> &str { &self.internals().name }
    #[inline]
    fn config(&self) -> &ModuleConfig { &self.internals().config }
    #[inline]
    fn req_receiver(&self) -> &Receiver<(HId, Msg)> { &self.internals().req_receiver }
    #[inline]
    fn rep_sender(&self) -> &Sender<(Option<HId>, Msg)> { &self.internals().rep_sender }

    fn run(mut self) where Self: Sized {
        mlzlog::set_thread_prefix(format!("[{}] ", self.name()));

        // TODO: decide whether to do polling here or in another thread
        let poll = tick(Duration::from_secs(1));

        loop {
            select! {
                recv(self.req_receiver()) -> res => if let Ok((hid, req)) = res {
                    let rep = match req {
                        Msg::ChangeReq { module, param, value } => match self.change(&param, value) {
                            Ok(value) => Msg::ChangeRep { module, param, value },
                            Err(e) => Msg::ErrorRep { class: "Error".into(),
                                                      // TODO
                                                      report: json!(["your request", "ERR", {}]) }
                        },
                        Msg::CommandReq { module, command, arg } => match self.command(&command, arg) {
                            Ok(result) => Msg::CommandRep { module, command, result },
                            Err(e) => Msg::ErrorRep { class: "Error".into(),
                                                      // TODO
                                                      report: json!(["your request", "ERR", {}]) }
                        },
                        Msg::TriggerReq { module, param } => match self.trigger(&param) {
                            Ok(value) => Msg::Update { module, param, value },
                            Err(e) => Msg::ErrorRep { class: "Error".into(),
                                                      // TODO
                                                      report: json!(["your request", "ERR", {}]) }
                        },
                        Msg::DescribeReq => {
                            Msg::DescribeRep { id: self.name().into(), desc: self.describe() }
                        },
                        _ => {
                            warn!("message should not arrive here: {}", req);
                            continue;
                        }
                    };
                    self.rep_sender().send((Some(hid), rep)).unwrap();
                },
                recv(poll) -> _ => {
                    for &param in self.poll_params() {
                        // TODO
                        if let Ok(value) = self.trigger(param) {
                            self.rep_sender().send((None, Msg::Update { module: self.name().into(),
                                                                        param: param.into(),
                                                                        value })).unwrap();
                        }
                    }
                }
            }
        }
    }
}
