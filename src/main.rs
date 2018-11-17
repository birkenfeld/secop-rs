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
//! The main entry point and crate definitions.

#![feature(duration_float)]

#![allow(unused_variables)]
#![allow(dead_code)]

#[macro_use]
extern crate log;
extern crate mlzlog;
extern crate mlzutil;
extern crate time;
extern crate fnv;
#[macro_use]
extern crate clap;
extern crate regex;
extern crate memchr;
#[macro_use]
extern crate lazy_static;
extern crate parking_lot;
extern crate daemonize;
extern crate chan_signal;
#[macro_use]
extern crate crossbeam_channel;
extern crate serde;
#[macro_use]
extern crate serde_json;
extern crate serde_derive;
// #[macro_use]
// extern crate secop_derive;

mod proto;
mod server;
mod module;
mod types;
mod errors;
mod util;

mod play;


fn main() {
    let args = clap_app!(("secop-rs") =>
        (version: crate_version!())
        (author: "")
        (about: "A Rust implementation of a SECoP server.")
        (@setting DeriveDisplayOrder)
        (@setting UnifiedHelpMessage)
        (@arg verbose: -v "Debug logging output?")
        (@arg bind: --bind [ADDR] default_value("127.0.0.1:10767") "Bind address (host:port)")
        (@arg log: --log [LOGPATH] default_value("log") "Logging path")
        (@arg pid: --pid [PIDPATH] default_value("pid") "PID path")
        (@arg daemon: -d "Daemonize?")
        (@arg user: --user [USER] "User name for daemon")
        (@arg group: --group [GROUP] "Group name for daemon")
        (@arg config: "Configuration file name to load") // TODO make it required
    ).get_matches();

    let log_path = util::abspath(args.value_of("log").expect(""));
    let pid_path = util::abspath(args.value_of("pid").expect(""));
    if args.is_present("daemon") {
        let mut daemon = daemonize::Daemonize::new();
        if let Some(user) = args.value_of("user") {
            daemon = daemon.user(user);
        }
        if let Some(group) = args.value_of("group") {
            daemon = daemon.group(group);
        }
        if let Err(err) = daemon.start() {
            eprintln!("could not daemonize process: {}", err);
        }
    }
    let config = args.value_of("config").unwrap_or("secop");
    if let Err(err) = mlzlog::init(Some(log_path), config, false,
                                   args.is_present("verbose"),
                                   !args.is_present("daemon")) {
        eprintln!("could not initialize logging: {}", err);
    }
    if let Err(err) = util::write_pidfile(&pid_path) {
        error!("could not write PID file: {}", err);
    }

    // handle SIGINT and SIGTERM
    let signal_chan = chan_signal::notify(&[chan_signal::Signal::INT,
                                            chan_signal::Signal::TERM]);

    let server = server::Server::new(config)
        .unwrap_or_else(|_| std::process::exit(1));
    let bind_addr = args.value_of("bind").expect("");
    info!("starting server on {}...", bind_addr);
    if let Err(err) = server.start(bind_addr) {
        error!("could not initialize server: {}", err);
    }

    // wait for a signal to finish
    signal_chan.recv().unwrap();
    info!("quitting...");
    util::remove_pidfile(pid_path);
}
