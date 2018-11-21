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

use crate::errors::Error;


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

pub const IDENT_REPLY: &str = "SINE2020&ISSE,SECoP,V2018-11-07,v1.0\\beta";

/// Enum that represents any message that can be sent over the network in the
/// protocol, and some others that are only used internally.
#[derive(Debug, Clone)]
pub enum Msg {
    /// identify request
    Idn,
    /// identify reply
    IdnReply { encoded: String },
    /// description request
    Describe,
    /// description reply
    Describing { id: String, structure: Value },
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
    Done { module: String, command: String, data: Value },
    /// change request
    Change { module: String, param: String, value: Value },
    /// change result
    Changed { module: String, param: String, data: Value },
    /// read request
    Read { module: String, param: String },
    /// heartbeat request
    Ping { token: String },
    /// heartbeat reply
    Pong { token: String, data: Value },
    /// error reply
    ErrMsg { class: String, report: Value },
    /// update event
    Update { module: String, param: String, data: Value },

    /// not a protocol message, but a collection of initial updates
    InitUpdates { module: String, updates: Vec<Msg> },
    /// not a protocol message, indicates the connection is done
    Quit,
}

/// An incoming message that carries around the originating line from the
/// client.  We need this line for the error message if something goes wrong.
#[derive(Clone)]
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
        match Self::parse_inner(&msg) {
            Ok(v) => Ok(IncomingMsg(msg, v)),
            Err(e) => Err(e.into_msg(msg)),
        }
    }

    fn parse_inner(msg: &str) -> Result<Msg, Error> {
        if let Some(captures) = MSG_RE.captures(msg) {
            let action = captures.get(1).expect("is required").as_str();

            let specifier = captures.get(2).map(|m| m.as_str()).unwrap_or("");
            let mut spec_split = specifier.splitn(2, ':').map(Into::into);
            let module = spec_split.next().expect("cannot be absent");
            let mut param = || spec_split.next().ok_or(Error::protocol("missing parameter"));

            let data = if let Some(jsonstr) = captures.get(3) {
                serde_json::from_str(jsonstr.as_str()).map_err(|_| Error::protocol("invalid JSON"))?
            } else {
                Value::Null
            };

            let parsed = match action {
                wire::READ =>       Read { module, param: param()? },
                wire::CHANGE =>     Change { module, param: param()?, value: data },
                wire::DO =>         Do { module, command: param()?, arg: data },
                wire::DESCRIBE =>   Describe,
                wire::ACTIVATE =>   Activate { module },
                wire::DEACTIVATE => Deactivate { module },
                wire::PING =>       Ping { token: specifier.into() },
                wire::IDN =>        Idn,
                wire::UPDATE =>     Update { module, param: param()?, data },
                wire::CHANGED =>    Changed { module, param: param()?, data },
                wire::DONE =>       Done { module, command: param()?, data },
                wire::DESCRIBING => Describing { id: specifier.into(), structure: data },
                wire::ACTIVE =>     Active { module },
                wire::INACTIVE =>   Inactive { module },
                wire::PONG =>       Pong { token: specifier.into(), data },
                wire::ERROR =>      ErrMsg { class: specifier.into(), report: data },
                _ => return Err(Error::protocol("no such message type"))
            };

            Ok(parsed)
        } else if msg == IDENT_REPLY {
            // identify reply has a special format (to be compatible with SCPI)
            Ok(IdnReply { encoded: IDENT_REPLY.into() })
        } else {
            Err(Error::protocol("invalid message format"))
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
            Update { module, param, data } =>
                write!(f, "{} {}:{} {}", wire::UPDATE, module, param, data),
            Changed { module, param, data } =>
                write!(f, "{} {}:{} {}", wire::CHANGED, module, param, data),
            Done { module, command, data } =>
                write!(f, "{} {}:{} {}", wire::DONE, module, command, data),
            Describing { id, structure } =>
                write!(f, "{} {} {}", wire::DESCRIBING, id, structure),
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
            InitUpdates { .. } => write!(f, "<updates>"),
            Quit => write!(f, "<eof>"),
        }
    }
}

impl fmt::Display for IncomingMsg {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
