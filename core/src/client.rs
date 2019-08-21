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
//! A client for use with internal and external modules.

use std::net::TcpStream;
use std::time::Duration;
use url::Url;
use crossbeam_channel::unbounded;
use serde_json::Value;

use crate::errors::{Error, Result};
use crate::server::{CON_SENDER, REQ_SENDER, next_handler_id,
                    HandlerId, ReqSender, RepReceiver};
use crate::proto::{IncomingMsg, Msg};


pub enum Client {
    Local(LocalClient),
    Remote(RemoteClient),
}

impl Client {
    pub fn new(addr: &str) -> Result<Self> {
        let baseurl = Url::parse("local://").expect("valid URL");
        match Url::options().base_url(Some(&baseurl)).parse(addr) {
            Err(e) => panic!("{}", e),
            Ok(uri) => match uri.scheme() {
                "local" => {
                    let loc = LocalClient::new(&uri.path()[1..]).ok_or_else(
                        || Error::comm_failed("no local server running"))?;
                    Ok(Client::Local(loc))
                }
                "secop" => {
                    let host = uri.domain().unwrap_or("localhost");
                    let port = uri.port().unwrap_or(10767);
                    let modname = uri.path()[1..].to_owned();
                    RemoteClient::new(host, port, modname).map(Client::Remote)
                }
                s => {
                    Err(Error::bad_value(format!("invalid URI scheme: {}", s)))
                }
            }
        }
    }

    pub fn ping(&self) -> Result<()> {
        match self {
            Client::Local(c) => c.ping(),
            Client::Remote(c) => c.ping()
        }
    }

    pub fn read(&self, param: &str) -> Result<Value> {
        match self {
            Client::Local(c) => c.read(param),
            Client::Remote(c) => c.read(param)
        }
    }

    pub fn change(&self, param: &str, value: Value) -> Result<Value> {
        match self {
            Client::Local(c) => c.change(param, value),
            Client::Remote(c) => c.change(param, value)
        }
    }

    pub fn command(&self, cmd: &str, arg: Value) -> Result<Value> {
        match self {
            Client::Local(c) => c.command(cmd, arg),
            Client::Remote(c) => c.command(cmd, arg)
        }
    }
}


/// Client that loops back requests to a module in this process.
pub struct LocalClient {
    hid: HandlerId,
    modname: String,
    timeout: Duration,
    req_sender: ReqSender,
    rep_receiver: RepReceiver,
}

impl Drop for LocalClient {
    fn drop(&mut self) {
        let _ = self.req_sender.send((self.hid, IncomingMsg::bare(Msg::Quit)));
    }
}

impl LocalClient {
    /// Return a new local client connecting to the given module.  None is
    /// returned if no local server is running.
    pub fn new(modname: impl Into<String>) -> Option<Self> {
        let timeout = Duration::from_secs(2); // TODO configurable
        let hid = next_handler_id();
        let con_sender = CON_SENDER.lock().clone()?;
        let req_sender = REQ_SENDER.lock().clone()?;
        let (rep_sender, rep_receiver) = unbounded();
        con_sender.send((hid, rep_sender.clone())).unwrap();
        Some(Self { hid, modname: modname.into(), timeout, req_sender, rep_receiver })
    }

    fn transact(&self, msg: Msg) -> Result<Msg> {
        self.req_sender.send((self.hid, IncomingMsg::bare(msg))).unwrap();
        match self.rep_receiver.recv_timeout(self.timeout) {
            Err(_)  => Err(Error::comm_failed("local module timed out")),
            Ok(msg) => Ok(msg)
        }
    }

    pub fn ping(&self) -> Result<()> {
        match self.transact(Msg::Ping { token: self.modname.clone() })? {
            Msg::Pong { ref token, .. } if token == &self.modname => Ok(()),
            msg => Err(Error::protocol(format!("invalid reply message for ping: {}", msg)))
        }
    }

    pub fn read(&self, param: &str) -> Result<Value> {
        let req = Msg::Read { module: self.modname.clone(), param: param.into() };
        match self.transact(req)? {
            Msg::Update { data, .. } => Ok(data), // TODO extract value from report
            msg => Err(Error::protocol(format!("invalid reply message for read: {}", msg)))
        }
    }

    pub fn change(&self, param: &str, value: Value) -> Result<Value> {
        let req = Msg::Change { module: self.modname.clone(), param: param.into(), value };
        match self.transact(req)? {
            Msg::Changed { data, .. } => Ok(data), // TODO extract value from report
            msg => Err(Error::protocol(format!("invalid reply message for change: {}", msg)))
        }
    }

    pub fn command(&self, cmd: &str, arg: Value) -> Result<Value> {
        let req = Msg::Do { module: self.modname.clone(), command: cmd.into(), arg: arg };
        match self.transact(req)? {
            Msg::Done { data, .. } => Ok(data), // TODO extract value from report
            msg => Err(Error::protocol(format!("invalid reply message for do: {}", msg)))
        }
    }
}

/// Client that accesses a module in some remote SEC node.
pub struct RemoteClient {
    _conn: TcpStream,
    modname: String,
}

impl RemoteClient {
    pub fn new(_host: &str, _port: u16, _modname: String) -> Result<Self> {
        Err(Error::config("remote client connection not yet implemented"))
    }

    fn transact(&self, _msg: Msg) -> Result<Msg> {
        unimplemented!()
    }

    pub fn ping(&self) -> Result<()> {
        match self.transact(Msg::Ping { token: self.modname.clone() })? {
            Msg::Pong { ref token, .. } if token == &self.modname => Ok(()),
            msg => Err(Error::protocol(format!("invalid reply message for ping: {}", msg)))
        }
    }

    pub fn read(&self, param: &str) -> Result<Value> {
        let req = Msg::Read { module: self.modname.clone(), param: param.into() };
        match self.transact(req)? {
            Msg::Update { data, .. } => Ok(data), // TODO extract value from report
            msg => Err(Error::protocol(format!("invalid reply message for read: {}", msg)))
        }
    }

    pub fn change(&self, param: &str, value: Value) -> Result<Value> {
        let req = Msg::Change { module: self.modname.clone(), param: param.into(), value };
        match self.transact(req)? {
            Msg::Changed { data, .. } => Ok(data), // TODO extract value from report
            msg => Err(Error::protocol(format!("invalid reply message for change: {}", msg)))
        }
    }

    pub fn command(&self, cmd: &str, arg: Value) -> Result<Value> {
        let req = Msg::Do { module: self.modname.clone(), command: cmd.into(), arg: arg };
        match self.transact(req)? {
            Msg::Done { data, .. } => Ok(data), // TODO extract value from report
            msg => Err(Error::protocol(format!("invalid reply message for do: {}", msg)))
        }
    }
}
