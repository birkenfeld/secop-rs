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
//! This module contains the server instance itself.

mod handler;

use std::io;
use std::net::{SocketAddr, TcpListener};
use std::thread;

use self::handler::Handler;

pub const RECVBUF_LEN: usize = 4096;

pub type ClientAddr = SocketAddr;

pub struct Server {
}

impl Server {
    pub fn new(config: &str) -> Result<Server, ()> {
        // create a channel to send updated keys to the updater thread
        //let (w_updates, r_updates) = unbounded();

        // start a thread that sends out updates to connected clients
        //thread::spawn(move || Server::updater(r_updates));

        Ok(Server { })
    }

    /// Listen for connections on the TCP socket and spawn handlers for it.
    fn tcp_listener(self, tcp_sock: TcpListener) {
        info!("tcp listener started");
        while let Ok((stream, addr)) = tcp_sock.accept() {
            info!("[{}] new client connected", addr);
            // create the handler and start its main thread
            thread::spawn(move || Handler::new(stream, addr).handle());
        }
    }

    /// Main server function; start threads to accept clients on the listening
    /// socket and spawn handlers to handle them.
    pub fn start(self, addr: &str) -> io::Result<()> {
        // create the TCP socket and start its handler thread
        let tcp_sock = TcpListener::bind(addr)?;
        thread::spawn(move || Server::tcp_listener(self, tcp_sock));
        Ok(())
    }
}
