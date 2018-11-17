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
//   Georg Brandl <georg.brandl@frm2.tum.de>
//
// -----------------------------------------------------------------------------
//
//! This module contains the definition of a protocol message, along with tools
//! to parse and string-format it.

use std::borrow::Cow;
use regex::Regex;
use json::{self, JsonValue};


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

type Str<'a> = Cow<'a, str>;

pub const IDENT_REPLY: &str = "SINE2020&ISSE,SECoP,V2018-02-13,rc2\n";

pub mod err {
    #![allow(unused)]
    use std::borrow::Cow;

    pub const NO_MODULE: Cow<'static, str> = Cow::Borrowed("NoSuchModule");
    pub const NO_PARAM:  Cow<'static, str> = Cow::Borrowed("NoSuchParameter");
    pub const NO_CMD:    Cow<'static, str> = Cow::Borrowed("NoSuchCommand");
    pub const CMD_FAIL:  Cow<'static, str> = Cow::Borrowed("CommandFailed");
    pub const READ_ONLY: Cow<'static, str> = Cow::Borrowed("ReadOnly");
    pub const BAD_VALUE: Cow<'static, str> = Cow::Borrowed("BadValue");
    pub const COMM_FAIL: Cow<'static, str> = Cow::Borrowed("CommunicationFailed");
    pub const IS_BUSY:   Cow<'static, str> = Cow::Borrowed("IsBusy");
    pub const IS_ERROR:  Cow<'static, str> = Cow::Borrowed("IsError");
    pub const PROTOCOL:  Cow<'static, str> = Cow::Borrowed("ProtocolError");
    pub const INTERNAL:  Cow<'static, str> = Cow::Borrowed("InternalError");
    pub const CMD_RUNS:  Cow<'static, str> = Cow::Borrowed("CommandRunning");
    pub const DISABLED:  Cow<'static, str> = Cow::Borrowed("Disabled");
}

/// An algebraic data type that represents any message that can be sent
/// over the network in the protocol.
///
/// String entries here can be borrowed from a network buffer, or owned.
#[derive(Debug)]
pub enum Msg<'a> {
    /// identify request
    IdentReq,
    /// identify reply
    IdentRep { encoded: Str<'a> },
    /// description request
    DescribeReq,
    /// description reply
    DescribeRep { id: Str<'a>, desc: JsonValue },
    /// event enable request
    EventEnableReq,
    /// event enable reply
    EventEnableRep,
    /// event disable request
    EventDisableReq,
    /// event disable reply
    EventDisableRep,
    /// command execution request
    CommandReq { module: Str<'a>, command: Option<Str<'a>>, args: JsonValue },
    /// command result
    CommandRep { module: Str<'a>, command: Option<Str<'a>>, result: JsonValue },
    /// change request
    WriteReq { module: Str<'a>, param: Option<Str<'a>>, value: JsonValue },
    /// change result
    WriteRep { module: Str<'a>, param: Option<Str<'a>>, value: JsonValue },
    /// trigger/poll request
    TriggerReq { module: Str<'a>, param: Option<Str<'a>> },
    /// heartbeat request
    PingReq { nonce: Str<'a> },
    /// heartbeat reply
    PingRep { nonce: Str<'a> },
    /// help request
    HelpReq,
    /// help reply
    HelpRep { n: u64, line: JsonValue },
    /// error reply
    ErrorRep { class: Str<'a>, info: JsonValue },
    /// event
    Event { module: Str<'a>, param: Option<Str<'a>>, value: JsonValue },
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
    pub const HELP: &str = "help";
    pub const HELPING: &str = "helping";
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

impl<'a> Msg<'a> {
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
                match json::parse(jsonstr.as_str()) {
                    Ok(v) => v,
                    Err(_) => return Err(ErrorRep { class: err::BAD_VALUE,
                                                    info: array!("invalid json",
                                                                 jsonstr.as_str()) })
                }
            } else { JsonValue::Null };
            match msgtype {
                wire::IDN =>        Ok(IdentReq),
                wire::DESCRIBE =>   Ok(DescribeReq),
                wire::DESCRIBING => Ok(DescribeRep { id: spec1, desc: data }),
                wire::ACTIVATE =>   Ok(EventEnableReq),
                wire::ACTIVE =>     Ok(EventEnableRep),
                wire::DEACTIVATE => Ok(EventDisableReq),
                wire::INACTIVE =>   Ok(EventDisableRep),
                wire::HELP =>       Ok(HelpReq),
                wire::HELPING =>    Ok(HelpRep { n: spec1.parse().unwrap_or(0), line: data }),
                wire::PING =>       Ok(PingReq { nonce: spec1 }),
                wire::PONG =>       Ok(PingRep { nonce: spec1 }),
                wire::ERROR =>      Ok(ErrorRep { class: spec1, info: data }),
                wire::DO =>         Ok(CommandReq { module: spec1, command: spec2, args: data }),
                wire::DONE =>       Ok(CommandRep { module: spec1, command: spec2, result: data }),
                wire::CHANGE =>     Ok(WriteReq { module: spec1, param: spec2, value: data }),
                wire::CHANGED =>    Ok(WriteRep { module: spec1, param: spec2, value: data }),
                wire::READ =>       Ok(TriggerReq { module: spec1, param: spec2 }),
                wire::UPDATE =>     Ok(Event { module: spec1, param: spec2, value: data }),
                _ => Err(ErrorRep { class: err::PROTOCOL,
                                    info: format!("{}: no such message type defined", msgtype).into() })
            }
        } else if msg == IDENT_REPLY {
            // identify reply has a special format (to be compatible with SCPI)
            Ok(Msg::IdentRep { encoded: IDENT_REPLY.into() })
        } else {
            // treat otherwise undecodable messages like a help request
            Ok(Msg::HelpReq)
        }
    }
}

/// "Serialize" a `Msg` back to a String.
///
/// Not all messages are actually used for stringification, but this is also
/// nice for debugging purposes.
impl<'a> ToString for Msg<'a> {
    fn to_string(&self) -> String {
        fn combine_spec<'a>(a: &Str<'a>, b: &Option<Str<'a>>) -> String {
            match *b {
                Some(ref b) => format!("{}:{}", a, b),
                None => a.to_string(),
            }
        }

        match self {
            IdentReq => wire::IDN.into(),
            IdentRep { encoded } => encoded.to_string(),
            DescribeReq => wire::DESCRIBE.into(),
            DescribeRep { id, desc } => format!("{} {} {}", wire::DESCRIBING, id, desc),
            EventEnableReq => wire::ACTIVATE.into(),
            EventEnableRep => wire::ACTIVE.into(),
            EventDisableReq => wire::DEACTIVATE.into(),
            EventDisableRep => wire::INACTIVE.into(),
            HelpReq => wire::HELP.into(),
            CommandReq { module, command, args } =>
                format!("{} {} {}", wire::DO, combine_spec(module, command), args),
            CommandRep { module, command, result } =>
                format!("{} {} {}", wire::DONE, combine_spec(module, command), result),
            WriteReq { module, param, value } =>
                format!("{} {} {}", wire::CHANGE, combine_spec(module, param), value),
            WriteRep { module, param, value } =>
                format!("{} {} {}", wire::CHANGED, combine_spec(module, param), value),
            TriggerReq { module, param } =>
                format!("{} {}", wire::READ, combine_spec(module, param)),
            PingReq { nonce } =>
                if nonce.is_empty() { wire::PING.into() } else { format!("{} {}", wire::PING, nonce) },
            PingRep { nonce } =>
                if nonce.is_empty() { wire::PONG.into() } else { format!("{} {}", wire::PONG, nonce) },
            HelpRep { n, line } =>
                format!("{} {} {}", wire::HELPING, n, line),
            ErrorRep { class, info } =>
                format!("{} {} {}", wire::ERROR, class, info),
            Event { module, param, value } =>
                format!("{} {} {}", wire::UPDATE, combine_spec(module, param), value),
        }
    }
}
