// -----------------------------------------------------------------------------
// A Rust implementation of the NICOS cache server.
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
//! This module contains misc. utilities.

#![allow(unused)]

use std::io::{self, Write};
use std::fs::{DirBuilder, OpenOptions, File, read_link, remove_file};
use std::path::{Path, PathBuf};

use time;


/// Local time as floating seconds since the epoch.
pub fn localtime() -> f64 {
    let ts = time::get_time();
    (ts.sec as f64) + ((ts.nsec as f64) / 1_000_000_000.)
}


/// Float time to timespec.
pub fn to_timespec(t: f64) -> time::Timespec {
    let itime = (1e9 * t) as u64;
    time::Timespec { nsec: (itime % 1_000_000_000) as i32,
                     sec:  (itime / 1_000_000_000) as i64 }
}


/// Time to floating.
pub fn to_timefloat(t: time::Tm) -> f64 {
    let ts = t.to_timespec();
    (ts.sec as f64) + ((ts.nsec as f64) / 1_000_000_000.)
}


/// mkdir -p utility.
pub fn ensure_dir<P: AsRef<Path>>(path: P) -> io::Result<()> {
    if path.as_ref().is_dir() {
        return Ok(());
    }
    DirBuilder::new().recursive(true).create(path)
}


/// Write a PID file.
pub fn write_pidfile<P: AsRef<Path>>(pid_path: P) -> io::Result<()> {
    let pid_path = pid_path.as_ref();
    ensure_dir(pid_path)?;
    let file = pid_path.join("cache_rs.pid");
    let my_pid = read_link("/proc/self")?;
    let my_pid = my_pid.as_os_str().to_str().unwrap().as_bytes();
    File::create(file)?.write_all(my_pid)?;
    Ok(())
}

/// Remove a PID file.
pub fn remove_pidfile<P: AsRef<Path>>(pid_path: P) {
    let file = Path::new(pid_path.as_ref()).join("cache_rs.pid");
    let _ = remove_file(file);
}


/// Shortcut for canonicalizing a path, if possible.
pub fn abspath<P: AsRef<Path>>(path: P) -> PathBuf {
    path.as_ref().canonicalize().unwrap_or_else(|_| path.as_ref().into())
}
