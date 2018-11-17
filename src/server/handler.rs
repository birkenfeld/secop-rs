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
//! This module contains the handler for a single network connection.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::thread;
use memchr::memchr;
use crossbeam_channel::{unbounded, Sender, Receiver};

use proto::IDENT_REPLY;
use proto::Msg;
use proto::Msg::*;
use server::{ClientAddr, RECVBUF_LEN};


pub struct Handler {
    name:   String,
    client: TcpStream,
    // addr:   ClientAddr,
    send_q: Sender<String>,
}

// TODO: use mlzlog::set_thread_name
// TODO: make names of channels more consistent

impl Handler {
    pub fn new(client: TcpStream, addr: ClientAddr) -> Handler {
        // spawn a thread that handles sending replies and events back
        let (w_msgs, r_msgs) = unbounded();
        let send_client = client.try_clone().expect("could not clone socket");
        let thread_name = addr.to_string();
        thread::spawn(move || Handler::sender(&thread_name, send_client, r_msgs));
        Handler {
            name:   addr.to_string(),
            // addr:   addr,
            send_q: w_msgs,
            client,
        }
    }

    /// Thread that sends back replies and events to the client.
    fn sender(name: &str, mut client: TcpStream, r_msgs: Receiver<String>) {
        for to_send in r_msgs {
            if let Err(err) = client.write_all(to_send.as_bytes()) {
                warn!("[{}] write error in sender: {}", name, err);
                break;
            }
        }
        info!("[{}] sender quit", name);
    }

    fn handle_msg(&self, msg: Msg) {
        match msg {
            IdentReq => {
                self.send_q.send(IDENT_REPLY.into()).expect("reply failed");
            }
            _ => {
                warn!("[{}] message {:?} not handled yet", self.name, msg);
            }
        }
    }

    /// Process a single line (message).
    fn process(&self, line: &str) -> bool {
        match Msg::parse(line) {
            Ok(msg) => {
                debug!("[{}] processing {:?} => {:?}", self.name, line, msg);
                self.handle_msg(msg);
                true
            }
            Err(e) => {
                // not a valid cache protocol line => ignore it
                warn!("[{}] unhandled line: {:?} - {}", self.name, line, e.to_string());
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
                    warn!("[{}] error in recv(): {}", self.name, err);
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
        info!("[{}] handler is finished", self.name);
    }
}
