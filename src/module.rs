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
use serde_json::Value;
use derive_new::new;
use crossbeam_channel::{Sender, Receiver, tick, select};

use crate::config::ModuleConfig;
use crate::errors::Error;
use crate::proto::{IncomingMsg, Msg};
use crate::server::HId;

/// Data that every module requires.
#[derive(new)]
pub struct ModInternals {
    name: String,
    config: ModuleConfig,
    req_receiver: Receiver<(HId, IncomingMsg)>,
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
    type PollParams: Default;

    fn change(&mut self, param: &str, value: Value) -> Result<Value, Error>;
    fn command(&mut self, cmd: &str, args: Value) -> Result<Value, Error>;
    fn trigger(&mut self, param: &str) -> Result<Value, Error>;
    fn describe(&self) -> Value;

    fn poll_normal(&mut self, n: usize, pp: &mut Self::PollParams);
    fn poll_busy(&mut self, n: usize, pp: &mut Self::PollParams);

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

    fn run(mut self) where Self: Sized {
        mlzlog::set_thread_prefix(format!("[{}] ", self.name()));

        // TODO: customizable poll interval
        let poll = tick(Duration::from_millis(1000));
        let poll_busy = tick(Duration::from_millis(200));

        let mut poll_params = Self::PollParams::default();
        let mut poll_normal_counter = 0usize;
        let mut poll_busy_counter = 0usize;

        loop {
            select! {
                recv(self.req_receiver()) -> res => if let Ok((hid, req)) = res {
                    let rep = match req.1 {
                        Msg::Change { module, param, value } => match self.change(&param, value) {
                            Ok(value) => Msg::Changed { module, param, value },
                            Err(e) => e.into_msg(req.0),
                        },
                        Msg::Do { module, command, arg } => match self.command(&command, arg) {
                            Ok(result) => Msg::Done { module, command, result },
                            Err(e) => e.into_msg(req.0),
                        },
                        Msg::Read { module, param } => match self.trigger(&param) {
                            Ok(value) => Msg::Update { module, param, value },
                            Err(e) => e.into_msg(req.0),
                        },
                        _ => {
                            warn!("message should not arrive here: {}", req);
                            continue;
                        }
                    };
                    self.rep_sender().send((Some(hid), rep)).unwrap();
                },
                recv(poll) -> _ => {
                    self.poll_normal(poll_normal_counter, &mut poll_params);
                    poll_normal_counter = poll_normal_counter.wrapping_add(1);
                },
                recv(poll_busy) -> _ => {
                    self.poll_busy(poll_busy_counter, &mut poll_params);
                    poll_busy_counter = poll_busy_counter.wrapping_add(1);
                }
            }
        }
    }
}
