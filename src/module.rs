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

// Basic params:
//
// Readable
// - value
// - pollinterval
// - status
// Writable
// - target
// Drivable
// - stop()
// Communicator
// - communicate(in) -> out

use std::time::Duration;
// use std::error::Error as StdError;
use serde_json::Value;
use crossbeam_channel::{Sender, Receiver, tick};

use crate::errors::Error;
use crate::proto::Msg;
use crate::server::HId;

pub type Config = ();

pub trait Module {
    fn create(config: &Config) -> Self where Self: Sized;
    fn get_api_description(&self) -> Value;
    fn change(&mut self, param: &str, value: Value) -> Result<Value, Error>;
    fn command(&mut self, cmd: &str, args: Value) -> Result<Value, Error>;
    fn trigger(&mut self, param: &str) -> Result<Value, Error>;

    fn run(mut self,
           name: String,
           req_receiver: Receiver<(HId, Msg)>,
           rep_sender: Sender<(Option<HId>, Msg)>)
        where Self: Sized
    {
        mlzlog::set_thread_prefix("[MOD] ".into()); // TODO need real name

        // TODO: decide whether to do polling here or in another thread
        let poll = tick(Duration::from_secs(1));

        loop {
            select! {
                recv(req_receiver) -> res => if let Ok((hid, req)) = res {
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
                        _ => {
                            warn!("message should not arrive here: {}", req);
                            continue;
                        }
                    };
                    rep_sender.send((Some(hid), rep)).unwrap();
                },
                recv(poll) -> _ => {
                    for &param in &["value", "setpoint", "status"] {
                        if let Ok(value) = self.trigger(param) {
                            rep_sender.send((None, Msg::Update { module: name.clone(),
                                                                 param: param.into(),
                                                                 value })).unwrap();
                        }
                    }
                }
            }
        }
    }
}
