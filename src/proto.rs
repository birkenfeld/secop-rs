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
//! This module contains the definition of a protocol message, along with tools
//! to parse and string-format it.

use std::fmt;
use regex::Regex;
use serde_json::Value;
use lazy_static::lazy_static;

use crate::errors::{Error, ErrorKind};


lazy_static! {
    static ref MSG_RE: Regex = Regex::new(r#"(?x)
    ^
    (?P<type>[*?\w]+)                 # message type (verb)
    (?: \s
      (?P<spec>[\w:<>]+)              # spec (object)
      (?: \s
        (?P<json>.*)                  # data (json)
      )?
    )?
    $
    "#).expect("valid regex");
}

pub const IDENT_REPLY: &str = "SINE2020&ISSE,SECoP,V2018-02-13,rc2";

/// An algebraic data type that represents any message that can be sent
/// over the network in the protocol.
#[derive(Debug, Clone)]
pub enum Msg {
    /// identify request
    Idn,
    /// identify reply
    IdnReply { encoded: String },
    /// description request
    Describe,
    /// description reply
    Describing { id: String, desc: Value },
    /// event enable request
    Activate { module: String },
    /// event enable reply
    Active { module: String },
    /// event disable request
    Deactivate { module: String },
    /// event disable reply
    Inactive { module: String },
    /// command execution request
    Do { module: String, command: String, arg: Value },
    /// command result
    Done { module: String, command: String, result: Value },
    /// change request
    Change { module: String, param: String, value: Value },
    /// change result
    Changed { module: String, param: String, value: Value },
    /// trigger/poll request
    Read { module: String, param: String },
    /// heartbeat request
    Ping { token: String },
    /// heartbeat reply
    Pong { token: String, data: Value },
    /// error reply
    ErrMsg { class: String, report: Value },
    /// update event
    Update { module: String, param: String, value: Value },
    /// not a protocol message, indicates the connection is done
    Quit,
}

pub struct IncomingMsg(pub String, pub Msg);

use self::Msg::*;

mod wire {
    pub const IDN: &str = "*IDN?";
    pub const DESCRIBE: &str = "describe";
    pub const DESCRIBING: &str = "describing";
    pub const ACTIVATE: &str = "activate";
    pub const ACTIVE: &str = "active";
    pub const DEACTIVATE: &str = "deactivate";
    pub const INACTIVE: &str = "inactive";
    pub const PING: &str = "ping";
    pub const PONG: &str = "pong";
    pub const ERROR: &str = "error";
    pub const DO: &str = "do";
    pub const DONE: &str = "done";
    pub const CHANGE: &str = "change";
    pub const CHANGED: &str = "changed";
    pub const READ: &str = "read";
    pub const UPDATE: &str = "update";
}

impl Msg {
    /// Parse a string slice containing a message.
    ///
    /// This matches a regular expression, and then creates a `Msg` if successful.
    pub fn parse(msg: String) -> Result<IncomingMsg, Msg> {
        if let Some(captures) = MSG_RE.captures(&msg) {
            let msgtype = captures.get(1).expect("is required").as_str();
            let mut split = captures.get(2).map(|m| m.as_str())
                                           .unwrap_or("").splitn(2, ':').map(Into::into);
            let spec1 = split.next().expect("cannot be absent");
            let spec2 = split.next();
            let data = if let Some(jsonstr) = captures.get(3) {
                match serde_json::from_str(jsonstr.as_str()) {
                    Ok(v) => v,
                    Err(_) => return Err(Error::new(ErrorKind::Protocol, "invalid JSON").into_msg(msg))
                }
            } else { Value::Null };
            let parsed = match msgtype {
                wire::READ =>       Read { module: spec1, param: spec2.expect("XXX") },
                wire::CHANGE =>     Change { module: spec1, param: spec2.expect("XXX"), value: data },
                wire::DO =>         Do { module: spec1, command: spec2.expect("XXX"), arg: data },
                wire::DESCRIBE =>   Describe,
                wire::ACTIVATE =>   Activate { module: spec1 },
                wire::DEACTIVATE => Deactivate { module: spec1 },
                wire::PING =>       Ping { token: spec1 },
                wire::IDN =>        Idn,
                wire::UPDATE =>     Update { module: spec1, param: spec2.expect("XXX"), value: data },
                wire::CHANGED =>    Changed { module: spec1, param: spec2.expect("XXX"), value: data },
                wire::DONE =>       Done { module: spec1, command: spec2.expect("XXX"), result: data },
                wire::DESCRIBING => Describing { id: spec1, desc: data },
                wire::ACTIVE =>     Active { module: spec1 },
                wire::INACTIVE =>   Inactive { module: spec1 },
                wire::PONG =>       Pong { token: spec1, data },
                wire::ERROR =>      ErrMsg { class: spec1, report: data },
                _ => return Err(Error::new(ErrorKind::Protocol, "no such message type").into_msg(msg))
            };
            Ok(IncomingMsg(msg, parsed))
        } else if msg == IDENT_REPLY {
            // identify reply has a special format (to be compatible with SCPI)
            Ok(IncomingMsg(msg, IdnReply { encoded: IDENT_REPLY.into() }))
        } else {
            Err(Error::new(ErrorKind::Protocol, "invalid message format").into_msg(msg))
        }
    }
}

/// "Serialize" a `Msg` back to a String.
///
/// Not all messages are actually used for stringification, but this is also
/// nice for debugging purposes.
impl fmt::Display for Msg {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Update { module, param, value } =>
                write!(f, "{} {}:{} {}", wire::UPDATE, module, param, value),
            Changed { module, param, value } =>
                write!(f, "{} {}:{} {}", wire::CHANGED, module, param, value),
            Done { module, command, result } =>
                write!(f, "{} {}:{} {}", wire::DONE, module, command, result),
            Describing { id, desc } =>
                write!(f, "{} {} {}", wire::DESCRIBING, id, desc),
            Active { module } =>
                if module.is_empty() { f.write_str(wire::ACTIVE) }
                else { write!(f, "{} {}", wire::ACTIVE, module) },
            Inactive { module } =>
                if module.is_empty() { f.write_str(wire::INACTIVE) }
                else { write!(f, "{} {}", wire::INACTIVE, module) },
            Pong { token, data } =>
                write!(f, "{} {} {}", wire::PONG, token, data),
            Idn => f.write_str(wire::IDN),
            IdnReply { encoded } => f.write_str(&encoded),
            Read { module, param } =>
                write!(f, "{} {}:{}", wire::READ, module, param),
            Change { module, param, value } =>
                write!(f, "{} {}:{} {}", wire::CHANGE, module, param, value),
            Do { module, command, arg } =>
                write!(f, "{} {}:{} {}", wire::DO, module, command, arg),
            Describe => f.write_str(wire::DESCRIBE),
            Activate { module } =>
                if module.is_empty() { f.write_str(wire::ACTIVATE) }
                else { write!(f, "{} {}", wire::ACTIVATE, module) },
            Deactivate { module } =>
                if module.is_empty() { f.write_str(wire::DEACTIVATE) }
                else { write!(f, "{} {}", wire::DEACTIVATE, module) },
            Ping { token } =>
                if token.is_empty() { f.write_str(wire::PING) }
                else { write!(f, "{} {}", wire::PING, token) },
            ErrMsg { class, report } =>
                write!(f, "{} {} {}", wire::ERROR, class, report),
            Quit => write!(f, "<eof>"),
        }
    }
}

impl fmt::Display for IncomingMsg {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
