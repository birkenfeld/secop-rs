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
//! This module contains the server instance itself.

use std::io::{self, prelude::*};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::num::NonZeroU64;
use std::thread;
use memchr::memchr;
use fxhash::{FxHashMap as HashMap, FxHashSet as HashSet};
use crossbeam_channel::{unbounded, Sender, Receiver};

use crate::module::Module;
use crate::proto::{Msg, Msg::*, IDENT_REPLY};
use crate::util::localtime;

pub const RECVBUF_LEN: usize = 4096;

pub type ClientAddr = SocketAddr;

pub type HId = NonZeroU64;

pub struct Server {
    modules: Vec<Box<dyn Module>>, // TODO unused now
}

impl Server {
    pub fn new(_config: &str) -> Result<Server, ()> {
        Ok(Server { modules: vec![] })
    }

    /// Listen for connections on the TCP socket and spawn handlers for it.
    fn tcp_listener(tcp_sock: TcpListener,
                    con_sender: Sender<(HId, Sender<String>)>,
                    req_sender: Sender<(HId, Msg)>)
    {
        mlzlog::set_thread_prefix("TCP: ".into());
        info!("listener started");
        let mut next_hid = 0;
        while let Ok((stream, addr)) = tcp_sock.accept() {
            next_hid += 1;
            info!("[{}] new client connected", addr);
            // create the handler and start its main thread
            let new_req_sender = req_sender.clone();
            let (rep_sender, rep_receiver) = unbounded();
            let disp_rep_sender = rep_sender.clone();
            let hid = NonZeroU64::new(next_hid).unwrap();
            con_sender.send((hid, disp_rep_sender)).unwrap();
            thread::spawn(move || Handler::new(hid, stream, addr,
                                               new_req_sender, rep_sender, rep_receiver).handle());
        }
    }

    /// Main server function; start threads to accept clients on the listening
    /// socket and spawn handlers to handle them.
    pub fn start(self, addr: &str) -> io::Result<()> {
        let (con_sender, con_receiver) = unbounded();
        let (req_sender, req_receiver) = unbounded();
        let (rep_sender, rep_receiver) = unbounded();

        // create the modules
        let mut mod_senders = HashMap::default();

        let (mod_sender, mod_receiver) = unbounded();
        let mod1 = crate::play::cryo::Cryo::create(&());
        mod_senders.insert("cryo".into(), mod_sender);
        let mod_rep_sender = rep_sender.clone();
        thread::spawn(move || mod1.run("cryo".into(), mod_receiver, mod_rep_sender));

        // create the dispatcher
        let dispatcher = Dispatcher {
            handlers: HashMap::default(),
            active: HashMap::default(),
            modules: mod_senders,
            connections: con_receiver,
            requests: req_receiver,
            replies: rep_receiver,
        };
        thread::spawn(move || dispatcher.run());

        // create the TCP socket and start its handler thread
        let tcp_sock = TcpListener::bind(addr)?;
        thread::spawn(move || Server::tcp_listener(tcp_sock, con_sender, req_sender));
        Ok(())
    }
}

struct Dispatcher {
    handlers: HashMap<HId, Sender<String>>,
    active: HashMap<String, HashSet<HId>>,
    modules: HashMap<String, Sender<(HId, Msg)>>,
    connections: Receiver<(HId, Sender<String>)>,
    requests: Receiver<(HId, Msg)>,
    replies: Receiver<(Option<HId>, Msg)>,
}

impl Dispatcher {
    fn run(mut self) {
        mlzlog::set_thread_prefix("Dispatcher: ".into());
        loop {
            select! {
                recv(self.connections) -> res => if let Ok((hid, conn)) = res {
                    // XXX: handlers are never cleaned up at the moment!
                    // info!("dispatcher got handler {}", hid);
                    self.handlers.insert(hid, conn);
                },
                recv(self.requests) -> res => if let Ok((hid, req)) = res {
                    // info!("dispatcher got request {} -> {}", hid, req);
                    match req {
                        CommandReq { ref module, .. } |
                        ChangeReq  { ref module, .. } |
                        TriggerReq { ref module, .. } => {
                            if let Some(chan) = self.modules.get(&**module) {
                                chan.send((hid, req)).unwrap();
                            }
                        }
                        EventEnableReq { module } => {
                            if !module.is_empty() {
                                self.active.entry(module.clone()).or_default().insert(hid);
                            } else {
                                for module in self.modules.keys() {
                                    self.active.entry(module.clone()).or_default().insert(hid);
                                }
                            }
                            self.handlers[&hid].send(format!("{}\n", EventEnableRep { module })).unwrap();
                        }
                        EventDisableReq { module } => {
                            if !module.is_empty() {
                                self.active.entry(module.clone()).or_default().remove(&hid);
                            } else {
                                for module in self.modules.keys() {
                                    self.active.entry(module.clone()).or_default().remove(&hid);
                                }
                            }
                            self.handlers[&hid].send(format!("{}\n", EventDisableRep { module })).unwrap();
                        }
                        DescribeReq => {
                            // TODO
                            self.handlers[&hid].send(format!("XXX\n")).unwrap();
                        }
                        _ => warn!("message should not arrive here: {}", req),
                    }
                },
                recv(self.replies) -> res => if let Ok((hid, rep)) = res {
                    // info!("dispatcher got reply {:?} -> {}", hid, rep);
                    match hid {
                        None => if let Update { ref module, .. } = rep {
                            if let Some(set) = self.active.get(&**module) {
                                for hid in set {
                                    self.handlers[hid].send(format!("{}\n", rep)).unwrap();
                                }
                            }
                        },
                        Some(hid) => if let Some(chan) = self.handlers.get(&hid) {
                            chan.send(format!("{}\n", rep)).unwrap();
                        }
                    }
                }
            }
        }
    }
}


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
