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

#![allow(unused_variables)]

mod util;
mod proto;
mod server;
mod config;
pub mod module;
#[macro_use]
pub mod types;
pub mod errors;

// module implementations to play around
mod play;

use log::*;
use clap::{clap_app, crate_version};


fn main() {
    let args = clap_app!(("secop-rs") =>
        (version: crate_version!())
        (author: "Georg Brandl, Enrico Faulhaber")
        (about: "A generic SECoP server.")
        (@setting DeriveDisplayOrder)
        (@setting UnifiedHelpMessage)
        (@arg verbose: -v "Debug logging output?")
        (@arg bind: --bind [ADDR] default_value("0.0.0.0:10767") "Bind address (host:port)")
        (@arg log: --log [LOGPATH] default_value("log") "Logging path")
        (@arg pid: --pid [PIDPATH] default_value("pid") "PID path")
        (@arg daemon: -d "Daemonize?")
        (@arg user: --user [USER] "User name for daemon")
        (@arg group: --group [GROUP] "Group name for daemon")
        (@arg config: +required "Configuration file name to load")
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

    // handle SIGINT and SIGTERM
    let signal_chan = chan_signal::notify(&[chan_signal::Signal::INT,
                                            chan_signal::Signal::TERM]);

    let cfgname = args.value_of("config").expect("is required");

    if let Err(err) = mlzlog::init(Some(log_path), cfgname, false,
                                   args.is_present("verbose"),
                                   !args.is_present("daemon")) {
        eprintln!("could not initialize logging: {}", err);
    }
    if let Err(err) = util::write_pidfile(&pid_path) {
        error!("could not write PID file: {}", err);
    }

    match config::load_config(cfgname) {
        Err(err) => error!("could not parse config file {}: {}", cfgname, err),
        Ok(cfg)  => {
            let server = server::Server::new(cfg);
            let bind_addr = args.value_of("bind").expect("");
            info!("starting server on {}...", bind_addr);
            if let Err(err) = server.start(bind_addr) {
                error!("could not initialize server: {}", err);
            } else {
                // server is running; wait for a signal to finish
                signal_chan.recv().expect("sender never closed");
            }
        }
    }

    info!("quitting...");
    util::remove_pidfile(pid_path);
}
