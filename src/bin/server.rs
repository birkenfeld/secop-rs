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
//! The main entry point for the server executable.

use log::*;
use clap::{clap_app, crate_version};
use mlzutil::fs as fsutil;

use secop_core::config;
use secop_core::server::Server;


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

    let log_path = fsutil::abspath(args.value_of("log").expect(""));
    let pid_path = fsutil::abspath(args.value_of("pid").expect(""));
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
    let signals = signal_hook::iterator::Signals::new(
        &[signal_hook::SIGINT, signal_hook::SIGTERM]).expect("signal register failed");

    let cfgname = args.value_of("config").expect("is required");

    if let Err(err) = mlzlog::init(Some(log_path), cfgname, false,
                                   args.is_present("verbose"),
                                   !args.is_present("daemon")) {
        eprintln!("could not initialize logging: {}", err);
    }
    if let Err(err) = fsutil::write_pidfile(&pid_path, cfgname) {
        error!("could not write PID file: {}", err);
    }

    // set a panic hook to log panics into the logfile
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let payload = if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.as_str()
        } else if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            s
        } else {
            "???"
        };
        if let Some(location) = panic_info.location() {
            error!("panic: {:?} ({})", payload, location);
        } else {
            error!("panic: {:?}", payload)
        }
        // call the original hook to get backtrace if requested
        default_hook(panic_info);
    }));

    // load the config and run!
    match config::load_config(cfgname) {
        Err(err) => error!("could not parse config file {}: {}", cfgname, err),
        Ok(cfg)  => {
            let server = Server::new(cfg);
            let bind_addr = args.value_of("bind").expect("");
            info!("starting server on {}...", bind_addr);
            if let Err(err) = server.start(bind_addr, secop_modules::run_module) {
                error!("could not initialize server: {}", err);
            } else {
                // server is running; wait for a signal to finish
                signals.wait();
            }
        }
    }

    info!("quitting...");
    fsutil::remove_pidfile(pid_path, cfgname);
}
