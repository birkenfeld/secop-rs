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
//! This module contains the server instance itself, and associated objects to
//! handle connections and message routing.

use std::error::Error as StdError;
use std::io::{Read as IoRead, Write as IoWrite};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::num::NonZeroU64;
use std::time::Duration;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use log::*;
use memchr::memchr;
use derive_new::new;
use fxhash::{FxHashMap as HashMap, FxHashSet as HashSet};
use crossbeam_channel::{unbounded, Sender, Receiver, select, tick};
use serde_json::{Value, json};
use mlzutil::time::localtime;
use parking_lot::Mutex;

use crate::config::ServerConfig;
use crate::errors::Error;
use crate::module::ModInternals;
use crate::proto::{IncomingMsg, Msg, Msg::*, IDENT_REPLY};

pub const RECVBUF_LEN: usize = 4096;
pub const MAX_MSG_LEN: usize = 1024*1024;

/// Handler ID.  This is nonzero so that Option<HandlerId> is the same size.
pub type HandlerId = NonZeroU64;

#[derive(new)]
pub struct Server {
    config: ServerConfig,
}

// Aliases for all the common channel types.
pub type ConSender = Sender<(HandlerId, RepSender)>;
pub type ConReceiver = Receiver<(HandlerId, RepSender)>;
pub type ReqSender = Sender<(HandlerId, IncomingMsg)>;
pub type ReqReceiver = Receiver<(HandlerId, IncomingMsg)>;
pub type RepSender = Sender<Msg>;
pub type RepReceiver = Receiver<Msg>;
pub type ModRepSender = Sender<(Option<HandlerId>, Msg)>;
pub type ModRepReceiver = Receiver<(Option<HandlerId>, Msg)>;

/// Global sender for new connections to the server.
pub static CON_SENDER: Mutex<Option<ConSender>> = Mutex::new(None);

/// Global sender for new requests to the dispatcher.
pub static REQ_SENDER: Mutex<Option<ReqSender>> = Mutex::new(None);

static NEXT_HID: AtomicUsize = AtomicUsize::new(1);

pub fn next_handler_id() -> HandlerId {
    NonZeroU64::new(NEXT_HID.fetch_add(1, Ordering::SeqCst) as u64).expect("is nonzero")
}

impl Server {
    /// Listen for connections on the TCP socket and spawn handlers for it.
    fn tcp_listener(tcp_sock: TcpListener) {
        mlzlog::set_thread_prefix("TCP: ");
        info!("listener started");
        let con_sender = CON_SENDER.lock().clone().expect("no server running?");
        while let Ok((stream, addr)) = tcp_sock.accept() {
            info!("[{}] new client connected", addr);
            // create the handler and start its main thread
            let new_req_sender = REQ_SENDER.lock().clone().expect("no server running?");
            let (rep_sender, rep_receiver) = unbounded();
            let disp_rep_sender = rep_sender.clone();
            let hid = next_handler_id();
            con_sender.send((hid, disp_rep_sender)).unwrap();
            thread::spawn(move || Handler::new(hid, stream, addr,
                                               new_req_sender, rep_sender, rep_receiver).handle());
        }
    }

    /// Main server function; start threads to accept clients on the listening
    /// socket, the dispatcher, and the individual modules.
    pub fn start<F>(mut self, addr: &str, mod_runner: F) -> Result<(), Box<StdError>>
        where F: Fn(ModInternals) -> Result<(), Box<StdError>>
    {
        // create a few channels we need for the dispatcher:
        // sending info about incoming connections to the dispatcher
        let (con_sender, con_receiver) = unbounded();
        *CON_SENDER.lock() = Some(con_sender);
        // sending requests from all handlers to the dispatcher
        let (req_sender, req_receiver) = unbounded();
        *REQ_SENDER.lock() = Some(req_sender);
        // sending replies from all modules to the dispatcher
        let (rep_sender, rep_receiver) = unbounded();

        // create the modules
        let mut active_sets = HashMap::default();
        let mut mod_senders = HashMap::default();

        for (name, modcfg) in self.config.modules.drain() {
            // channel to send requests to the module
            let (mod_sender, mod_receiver) = unbounded();
            // replies go via a single one
            let mod_rep_sender = rep_sender.clone();
            let tickers = (tick(Duration::from_secs(1)), tick(Duration::from_secs(1)));
            let int = ModInternals::new(name.clone(), modcfg, mod_receiver, mod_rep_sender, tickers);
            active_sets.insert(name.clone(), HashSet::default());
            mod_senders.insert(name, mod_sender);
            mod_runner(int)?;
        }

        let descriptive = json!({
            "description": self.config.description,
            "equipment_id": self.config.equipment_id,
            "firmware": "secop-rs",
            "modules": []
        });

        // create the dispatcher
        let dispatcher = Dispatcher {
            descriptive: descriptive,
            active: active_sets,
            handlers: HashMap::default(),
            modules: mod_senders,
            connections: con_receiver,
            requests: req_receiver,
            replies: rep_receiver,
        };
        thread::spawn(move || dispatcher.run());

        // create the TCP socket and start its handler thread
        let tcp_sock = TcpListener::bind(addr)?;
        thread::spawn(move || Server::tcp_listener(tcp_sock));
        Ok(())
    }
}

/// The dispatcher acts as a central piece connected to both modules and clients,
/// all via channels.
struct Dispatcher {
    descriptive: Value,
    handlers: HashMap<HandlerId, RepSender>,
    active: HashMap<String, HashSet<HandlerId>>,
    modules: HashMap<String, ReqSender>,
    connections: ConReceiver,
    requests: ReqReceiver,
    replies: ModRepReceiver,
}

impl Dispatcher {
    fn send_back(&self, hid: HandlerId, msg: Msg) {
        if let Some(chan) = self.handlers.get(&hid) {
            let _ = chan.send(msg);
        }
    }

    fn run(mut self) {
        mlzlog::set_thread_prefix("Dispatcher: ");

        // > 0 if a global activation is currently being processed.
        let mut global_activate_remaining = 0;

        loop {
            select! {
                recv(self.connections) -> res => if let Ok((hid, conn)) = res {
                    debug!("got handler {}", hid);
                    self.handlers.insert(hid, conn);
                },
                recv(self.requests) -> res => if let Ok((hid, req)) = res {
                    debug!("got request {} -> {}", hid, req);
                    match req.1 {
                        Do { ref module, .. } |
                        Change { ref module, .. } |
                        Read { ref module, .. } => {
                            // check if module exists
                            if let Some(chan) = self.modules.get(module) {
                                chan.send((hid, req)).unwrap();
                            } else {
                                self.send_back(hid, Error::no_module().into_msg(req.0));
                            }
                        }
                        Activate { ref module } => {
                            // The activate message requires an "update" of all parameters
                            // to be sent before "active".  Other events should not be sent.
                            // To do this, we send this on to the module / all modules.
                            //
                            // When all replies arrived, we trigger the Active message.
                            if !module.is_empty() {
                                // check if module exists, send message on to it
                                if let Some(chan) = self.modules.get(module) {
                                    chan.send((hid, req)).unwrap();
                                } else {
                                    self.send_back(hid, Error::no_module().into_msg(req.0));
                                    continue;
                                }
                            } else {
                                // this is a global activation
                                if global_activate_remaining > 0 {
                                    // only one can be inflight
                                    self.send_back(hid, Error::protocol(
                                        "already activating").into_msg(req.0));
                                    continue;
                                }
                                // send this on to all modules - the "module" entry
                                // (which is empty here) will be replicated in the
                                // responding InitUpdates message
                                for chan in self.modules.values() {
                                    chan.send((hid, req.clone())).unwrap();
                                }
                                global_activate_remaining = self.modules.len();
                            }
                        }
                        Deactivate { module } => {
                            // Deactivation is done instantly, much easier than activation.
                            if !module.is_empty() {
                                // check if module exists
                                if !self.modules.contains_key(&module) {
                                    self.send_back(hid, Error::no_module().into_msg(req.0));
                                    continue;
                                }
                                self.active.get_mut(&module).expect("always there").remove(&hid);
                            } else {
                                // remove handler as active from all modules
                                for module in self.modules.keys() {
                                    self.active.get_mut(module).expect("always there").remove(&hid);
                                }
                            }
                            self.send_back(hid, Inactive { module });
                        }
                        Describe => {
                            self.send_back(hid, Describing {
                                id: ".".into(),
                                structure: self.descriptive.clone()
                            });
                        }
                        Quit => {
                            // the handler has quit - also remove it from all active lists
                            self.handlers.remove(&hid);
                            for set in self.active.values_mut() {
                                set.remove(&hid);
                            }
                        }
                        _ => warn!("message should not arrive here: {}", req.1),
                    }
                },
                recv(self.replies) -> res => if let Ok((hid, rep)) = res {
                    match hid {
                        None => match rep {
                            // update of descriptive data, isn't sent on to clients
                            // but cached here
                            Describing { id, structure } => {
                                let arr = self.descriptive["modules"].as_array_mut().expect("array");
                                match arr.iter_mut().find(|item| item[0] == id) {
                                    Some(item) => *item = structure,
                                    None => arr.push(structure)
                                }
                            }
                            // event update from a module, check where to send it
                            Update { ref module, .. } => {
                                debug!("got {}", rep);
                                for &hid in &self.active[module] {
                                    self.send_back(hid, rep.clone());
                                }
                            }
                            _ => ()
                        },
                        // specific reply from a module
                        Some(hid) => match rep {
                            InitUpdates { module, updates } => {
                                for msg in updates {
                                    self.send_back(hid, msg);
                                }
                                if !module.is_empty() {
                                    self.send_back(hid, Active { module: module.clone() });
                                    self.active.get_mut(&module).expect("always there").insert(hid);
                                } else {
                                    global_activate_remaining -= 1;
                                    if global_activate_remaining == 0 {
                                        self.send_back(hid, Active { module: "".into() });
                                        for set in self.active.values_mut() {
                                            set.insert(hid);
                                        }
                                    }
                                }
                            }
                            _ => {
                                debug!("got reply {} for {}", rep, hid);
                                self.send_back(hid, rep)
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The Handler represents a single client connection, both the read and
/// write halves.
///
/// The write half is in its own thread to be able to send back replies (which
/// can come both from the Handler and the Dispatcher) instantly.
pub struct Handler {
    client: TcpStream,
    /// Assigned handler ID.
    hid: HandlerId,
    /// Sender for incoming requests, to the dispatcher.
    req_sender: ReqSender,
    /// Sender for outgoing replies, to the sender thread.
    rep_sender: RepSender,
}

impl Handler {
    pub fn new(hid: HandlerId, client: TcpStream, addr: SocketAddr, req_sender: ReqSender,
               rep_sender: RepSender, rep_receiver: RepReceiver) -> Handler {
        // spawn a thread that handles sending replies and events back
        let send_client = client.try_clone().expect("could not clone socket");
        let thread_name = addr.to_string();
        thread::spawn(move || Handler::sender(&thread_name, send_client, rep_receiver));
        mlzlog::set_thread_prefix(format!("[{}] ", addr));
        Handler { hid, client, req_sender, rep_sender }
    }

    /// Thread that sends back replies and events to the client.
    fn sender(name: &str, client: TcpStream, rep_receiver: RepReceiver) {
        mlzlog::set_thread_prefix(format!("[{}] ", name));
        let mut client = std::io::BufWriter::new(client);
        for to_send in rep_receiver {
            if let Err(err) = write!(client, "{}\n", to_send) {
                warn!("write error in sender: {}", err);
                break;
            }
            let _ = client.flush();
        }
        info!("sender quit");
    }

    /// Send a message back to the client.
    fn send_back(&self, msg: Msg) {
        self.rep_sender.send(msg).expect("sending to client failed");
    }

    /// Handle an incoming correctly-parsed message.
    fn handle_msg(&self, msg: IncomingMsg) {
        match msg.1 {
            // most messages must go through the dispatcher to a module
            Change { .. } | Do { .. } | Read { .. } | Describe |
            Activate { .. } | Deactivate { .. } => {
                self.req_sender.send((self.hid, msg)).unwrap();
            }
            // but a few of them we can respond to from here
            Ping { token } => {
                let data = json!([null, {"t": localtime()}]);
                self.send_back(Pong { token, data });
            }
            Idn => {
                self.send_back(IdnReply { encoded: IDENT_REPLY.into() });
            }
            _ => {
                warn!("message {:?} not handled yet", msg.1);
            }
        }
    }

    /// Process a single line (message).
    fn process(&self, line: String) {
        match Msg::parse(line) {
            Ok(msg) => {
                debug!("processing {}", msg);
                self.handle_msg(msg);
            }
            Err(msg) => {
                // error while parsing: msg will be an ErrorRep
                warn!("failed to parse line: {}", msg);
                self.send_back(msg);
            }
        }
    }

    /// Handle incoming stream of messages.
    pub fn handle(mut self) {
        let mut buf = Vec::with_capacity(RECVBUF_LEN);
        let mut recvbuf = [0u8; RECVBUF_LEN];

        loop {
            // read a chunk of incoming data
            let got = match self.client.read(&mut recvbuf) {
                Err(err) => {
                    warn!("error in recv, closing connection: {}", err);
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
                let line_str = String::from_utf8_lossy(&buf[from..from+to]);
                let line_str = line_str.trim_right_matches('\r');
                self.process(line_str.to_owned());
                from += to + 1;
            }
            buf.drain(..from);
            // limit the incoming request length
            if buf.len() > MAX_MSG_LEN {
                warn!("hit request length limit, closing connection");
                break;
            }
        }
        self.req_sender.send((self.hid, IncomingMsg::bare(Quit))).unwrap();
        info!("handler is finished");
    }
}
