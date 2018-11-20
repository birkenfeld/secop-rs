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
use serde_json::{Value, json};
use lazy_static::lazy_static;


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
    "#).unwrap();
}

type Str = String;

pub const IDENT_REPLY: &str = "SINE2020&ISSE,SECoP,V2018-02-13,rc2";

/// An algebraic data type that represents any message that can be sent
/// over the network in the protocol.
///
/// String entries here can be borrowed from a network buffer, or owned.
#[derive(Debug, Clone)]
pub enum Msg {
    /// identify request
    IdentReq,
    /// identify reply
    IdentRep { encoded: Str },
    /// description request
    DescribeReq,
    /// description reply
    DescribeRep { id: Str, desc: Value },
    /// event enable request
    EventEnableReq { module: Str },
    /// event enable reply
    EventEnableRep { module: Str },
    /// event disable request
    EventDisableReq { module: Str },
    /// event disable reply
    EventDisableRep { module: Str },
    /// command execution request
    CommandReq { module: Str, command: Str, arg: Value },
    /// command result
    CommandRep { module: Str, command: Str, result: Value },
    /// change request
    ChangeReq { module: Str, param: Str, value: Value },
    /// change result
    ChangeRep { module: Str, param: Str, value: Value },
    /// trigger/poll request
    TriggerReq { module: Str, param: Str },
    /// heartbeat request
    PingReq { token: Str },
    /// heartbeat reply
    PingRep { token: Str, data: Value },
    /// error reply
    ErrorRep { class: Str, report: Value },
    /// update event
    Update { module: Str, param: Str, value: Value },
    /// not a protocol message, indicates the connection is done
    Quit,
}

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
    pub fn parse(msg: &str) -> Result<Msg, Msg> {
        if let Some(captures) = MSG_RE.captures(msg) {
            let msgtype = captures.get(1).unwrap().as_str();
            let mut split = captures.get(2).map(|m| m.as_str())
                                           .unwrap_or("").splitn(2, ':').map(Into::into);
            let spec1 = split.next().unwrap();
            let spec2 = split.next();
            let data = if let Some(jsonstr) = captures.get(3) {
                match serde_json::from_str(jsonstr.as_str()) {
                    Ok(v) => v,
                    Err(_) => return Err(ErrorRep { class: "BadValue".into(), // TODO dupe
                                                    report: json!([msg, "invalid json", {}]) })
                }
            } else { Value::Null };
            match msgtype {
                wire::IDN =>        Ok(IdentReq),
                wire::DESCRIBE =>   Ok(DescribeReq),
                wire::DESCRIBING => Ok(DescribeRep { id: spec1, desc: data }),
                wire::ACTIVATE =>   Ok(EventEnableReq { module: spec1 }),
                wire::ACTIVE =>     Ok(EventEnableRep { module: spec1 }),
                wire::DEACTIVATE => Ok(EventDisableReq { module: spec1 }),
                wire::INACTIVE =>   Ok(EventDisableRep { module: spec1 }),
                wire::PING =>       Ok(PingReq { token: spec1 }),
                wire::PONG =>       Ok(PingRep { token: spec1, data: data }),
                wire::ERROR =>      Ok(ErrorRep { class: spec1, report: data }),
                wire::DO =>         Ok(CommandReq { module: spec1, command: spec2.expect("XXX"), arg: data }),
                wire::DONE =>       Ok(CommandRep { module: spec1, command: spec2.expect("XXX"), result: data }),
                wire::CHANGE =>     Ok(ChangeReq { module: spec1, param: spec2.expect("XXX"), value: data }),
                wire::CHANGED =>    Ok(ChangeRep { module: spec1, param: spec2.expect("XXX"), value: data }),
                wire::READ =>       Ok(TriggerReq { module: spec1, param: spec2.expect("XXX") }),
                wire::UPDATE =>     Ok(Update { module: spec1, param: spec2.expect("XXX"), value: data }),
                _ => Err(ErrorRep { class: "ProtocolError".into(), // TODO dupe
                                    report: json!([msg, "no such message type", {}]) })
            }
        } else if msg == IDENT_REPLY {
            // identify reply has a special format (to be compatible with SCPI)
            Ok(IdentRep { encoded: IDENT_REPLY.into() })
        } else {
            // treat otherwise undecodable messages like a help request
            Err(ErrorRep { class: "ProtocolError".into(),
                           report: json!([msg, "invalid message format", {}]) })
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
            IdentReq => f.write_str(wire::IDN),
            IdentRep { encoded } => f.write_str(&encoded),
            DescribeReq => f.write_str(wire::DESCRIBE),
            DescribeRep { id, desc } => write!(f, "{} {} {}", wire::DESCRIBING, id, desc),
            EventEnableReq { module } =>
                if module.is_empty() { f.write_str(wire::ACTIVATE) }
                else { write!(f, "{} {}", wire::ACTIVATE, module) },
            EventEnableRep { module } =>
                if module.is_empty() { f.write_str(wire::ACTIVE) }
                else { write!(f, "{} {}", wire::ACTIVE, module) },
            EventDisableReq { module } =>
                if module.is_empty() { f.write_str(wire::DEACTIVATE) }
                else { write!(f, "{} {}", wire::DEACTIVATE, module) },
            EventDisableRep { module } =>
                if module.is_empty() { f.write_str(wire::INACTIVE) }
                else { write!(f, "{} {}", wire::INACTIVE, module) },
            CommandReq { module, command, arg } =>
                write!(f, "{} {}:{} {}", wire::DO, module, command, arg),
            CommandRep { module, command, result } =>
                write!(f, "{} {}:{} {}", wire::DONE, module, command, result),
            ChangeReq { module, param, value } =>
                write!(f, "{} {}:{} {}", wire::CHANGE, module, param, value),
            ChangeRep { module, param, value } =>
                write!(f, "{} {}:{} {}", wire::CHANGED, module, param, value),
            TriggerReq { module, param } =>
                write!(f, "{} {}:{}", wire::READ, module, param),
            Update { module, param, value } =>
                write!(f, "{} {}:{} {}", wire::UPDATE, module, param, value),
            PingReq { token } =>
                if token.is_empty() { f.write_str(wire::PING) }
                else { write!(f, "{} {}", wire::PING, token) },
            PingRep { token, data } => write!(f, "{} {} {}", wire::PONG, token, data),
            ErrorRep { class, report } =>
                write!(f, "{} {} {}", wire::ERROR, class, report),
            Quit => write!(f, "<eof>"),
        }
    }
}
