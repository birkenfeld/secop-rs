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
//! This module contains the handler for a single network connection.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::thread;
use memchr::memchr;
use crossbeam_channel::{Sender, Receiver};

use crate::proto::IDENT_REPLY;
use crate::proto::Msg;
use crate::proto::Msg::*;
use crate::server::{ClientAddr, RECVBUF_LEN, HId};
use crate::util::localtime;


pub struct Handler {
    hid: HId,
    addr: ClientAddr,
    client: TcpStream,
    req_sender: Sender<(HId, Msg)>,
    rep_sender: Sender<String>,
}

impl Handler {
    pub fn new(hid: HId, client: TcpStream, addr: ClientAddr,
               req_sender: Sender<(HId, Msg)>,
               rep_sender: Sender<String>, rep_receiver: Receiver<String>) -> Handler {
        // spawn a thread that handles sending replies and events back
        let send_client = client.try_clone().expect("could not clone socket");
        let thread_name = addr.to_string();
        thread::spawn(move || Handler::sender(&thread_name, send_client, rep_receiver));
        mlzlog::set_thread_prefix(format!("[{}] ", addr));
        Handler { hid, addr, client, req_sender, rep_sender }
    }

    /// Thread that sends back replies and events to the client.
    fn sender(name: &str, mut client: TcpStream, rep_receiver: Receiver<String>) {
        mlzlog::set_thread_prefix(format!("[{}] ", name));
        for to_send in rep_receiver {
            if let Err(err) = client.write_all(to_send.as_bytes()) {
                warn!("write error in sender: {}", err);
                break;
            }
        }
        info!("sender quit");
    }

    fn send_back(&self, msg: Msg) {
        self.rep_sender.send(format!("{}\n", msg)).expect("sending to client failed");
    }

    fn handle_msg(&self, msg: Msg) {
        match msg {
            ChangeReq { .. } | CommandReq { .. } | TriggerReq { .. } | DescribeReq |
            EventEnableReq { .. } | EventDisableReq { .. } => {
                self.req_sender.send((self.hid, msg)).unwrap();
            }
            PingReq { token } => {
                let data = json!([null, {"t": localtime()}]);
                self.send_back(PingRep { token, data });
            }
            IdentReq => {
                self.send_back(IdentRep { encoded: IDENT_REPLY.into() });
            }
            _ => {
                warn!("message {:?} not handled yet", msg);
            }
        }
    }

    /// Process a single line (message).
    fn process(&self, line: &str) -> bool {
        match Msg::parse(line) {
            Ok(msg) => {
                debug!("processing {:?} => {:?}", line, msg);
                self.handle_msg(msg);
                true
            }
            Err(msg) => {
                // error while parsing: this will be an ErrorRep
                warn!("failed to parse line: {:?} - {}", line, msg);
                self.send_back(msg);
                true
            }
        }
    }

    /// Handle incoming stream of messages.
    pub fn handle(mut self) {
        let mut buf = Vec::with_capacity(RECVBUF_LEN);
        let mut recvbuf = [0u8; RECVBUF_LEN];

        'outer: loop {
            // read a chunk of incoming data
            let got = match self.client.read(&mut recvbuf) {
                Err(err) => {
                    warn!("error in recv(): {}", err);
                    break;
                },
                Ok(0)    => break,  // no data from blocking read...
                Ok(got)  => got,
            };
            // convert to string and add to our buffer
            buf.extend_from_slice(&recvbuf[..got]);
            // process all whole lines we got
            let mut from = 0;
            while let Some(to) = memchr(b'\n', &buf[from..]) {
                // note, this won't allocate a new String if valid UTF-8
                let line_str = String::from_utf8_lossy(&buf[from..from+to]);
                let line_str = line_str.trim_right_matches('\r');
                if !self.process(line_str) {
                    // false return value means "quit"
                    break 'outer;
                }
                from += to + 1;
            }
            buf.drain(..from);
        }
        info!("handler is finished");
    }
}
