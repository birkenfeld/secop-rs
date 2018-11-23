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
//! Enumeration of possible SECoP errors.

use std::{error, fmt, result};
use serde_json::json;

use crate::proto::Msg;


pub type Result<T> = result::Result<T, Error>;

#[derive(Debug)]
pub enum ErrorKind {
    // Internal
    Config,
    Programming,
    Parsing,
    // API defined
    Protocol,
    NoSuchModule,
    NoSuchParameter,
    NoSuchCommand,
    CommandFailed,
    CommandRunning,
    ReadOnly,
    BadValue,
    CommunicationFailed,
    Timeout,       // ATM also C.F.
    HardwareError, // ATM also C.F.
    IsBusy,
    IsError,
    Disabled,
}

#[derive(Debug)]
pub struct Error {
    kind: ErrorKind,
    message: String,
}

impl Error {
    pub fn new(kind: ErrorKind, msg: impl Into<String>) -> Self {
        Self { kind, message: msg.into() }
    }

    pub fn bad_value(msg: impl Into<String>) -> Self {
        Self { kind: ErrorKind::BadValue, message: msg.into() }
    }

    pub fn amend(mut self, msg: &str) -> Self {
        self.message = format!("{} ({})", self.message, msg);
        self
    }

    pub fn into_msg(self, msg: String) -> Msg {
        Msg::ErrMsg {
            class: error::Error::description(&self).into(),
            report: json!([msg, self.message, {}])
        }
    }

    // Quick construction.

    pub fn config(msg: impl Into<String>) -> Self {
        Self { kind: ErrorKind::Config, message: msg.into() }
    }

    pub fn protocol(msg: impl Into<String>) -> Self {
        Self { kind: ErrorKind::Protocol, message: msg.into() }
    }

    pub fn no_module() -> Self {
        Self { kind: ErrorKind::NoSuchModule, message: "".into() }
    }

    pub fn no_param() -> Self {
        Self { kind: ErrorKind::NoSuchParameter, message: "".into() }
    }

    pub fn no_command() -> Self {
        Self { kind: ErrorKind::NoSuchCommand, message: "".into() }
    }

    pub fn comm_failed(msg: impl Into<String>) -> Self {
        Self { kind: ErrorKind::CommunicationFailed, message: msg.into() }
    }
}

/// Allow quick conversion of io::Error to SECoP errors.
// TODO: is comm_failed the right kind?
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::comm_failed(e.to_string())
    }
}

impl error::Error for Error {
    /// This is also the wire format of the error kind.
    fn description(&self) -> &str {
        use self::ErrorKind::*;
        match self.kind {
            Config | Programming | Parsing => "InternalError",
            Protocol => "ProtocolError",
            NoSuchModule => "NoSuchModule",
            NoSuchParameter => "NoSuchParameter",
            NoSuchCommand => "NoSuchCommand",
            CommandFailed => "CommandFailed",
            CommandRunning => "CommandRunning",
            ReadOnly => "ReadOnly",
            BadValue => "BadValue",
            CommunicationFailed => "CommunicationFailed",
            Timeout => "CommunicationFailed",
            HardwareError => "CommunicationFailed",
            IsBusy => "IsBusy",
            IsError => "IsError",
            Disabled => "Disabled",
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {}", error::Error::description(self), self.message)
    }
}
