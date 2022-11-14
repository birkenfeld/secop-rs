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
use mlzutil::fs as fsutil;
use clap::Parser;

use secop_core::config;
use secop_core::server::Server;


#[derive(Parser)]
struct Options {
    #[clap(short='v', long="verbose", help="Debug logging output?")]
    verbose: bool,
    #[clap(long="log", help="Logging path (if not given, log to journal)")]
    log: Option<String>,
    #[clap(long="bind", help="Bind address (host:port)", default_value="0.0.0.0:10767")]
    bind: String,
    #[clap(help="Configuration file name to load")]
    config: String,
}


fn main() {
    let opts = Options::from_args();

    let log_path = opts.log.as_ref().map(|l| fsutil::abspath(l));
    let log_console = log_path.is_none();

    // handle SIGINT and SIGTERM
    let mut signals = signal_hook::iterator::Signals::new(
        &[signal_hook::consts::signal::SIGINT,
          signal_hook::consts::signal::SIGTERM]).expect("signal register failed");

    if let Err(err) = mlzlog::init(log_path, "", mlzlog::Settings {
        show_appname: false,
        debug: opts.verbose,
        use_stdout: log_console,
        .. Default::default()
    }) {
        eprintln!("could not initialize logging: {}", err);
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
    match config::load_config(&opts.config) {
        Err(err) => error!("could not parse config file {}: {}", opts.config, err),
        Ok(cfg)  => {
            let server = Server::new(cfg);
            info!("starting server on {}...", opts.bind);
            if let Err(err) = server.start(&opts.bind, secop_modules::run_module) {
                error!("could not initialize server: {}", err);
            } else {
                // server is running; wait for a signal to finish
                signals.wait();
            }
        }
    }

    info!("quitting...");
}
