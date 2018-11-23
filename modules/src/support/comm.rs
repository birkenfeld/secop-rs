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
//! A generic "communicator" thread.

use std::io::{Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;
use log::*;
use parking_lot::{Condvar, Mutex, MutexGuard};

use secop_core::errors::{Error, Result};

pub type Connector<R, W> = Box<FnMut() -> Result<(R, W)> + Send + 'static>;

struct CommShared<W> {
    writer: Mutex<W>,
    buffer: Mutex<Vec<u8>>,
    seen_eol: Condvar,
    connected: AtomicBool,
    stop: AtomicBool,
}

pub struct CommThread<R, W> {
    connect: Connector<R, W>,
    reader: R,
    eol: Vec<u8>,
    shared: Arc<CommShared<W>>,
}

pub struct CommClient<W> {
    sol: Vec<u8>,
    eol: Vec<u8>,
    shared: Arc<CommShared<W>>,
    timeout: Duration,
}

impl<W: Write> CommClient<W> {
    fn get_line(&self, mut buffer: MutexGuard<Vec<u8>>) -> Result<Vec<u8>> {
        let neol = self.eol.len();
        let end = buffer
            .windows(neol)
            .position(|ch| ch == &self.eol[..])
            .unwrap();
        Ok(buffer.drain(0..end + neol).take(end).collect())
    }

    pub fn read(&self, max: u32) -> Result<Vec<u8>> {
        let mut buffer = self.shared.buffer.lock();
        let cur = buffer.len();
        let drain = buffer.drain(0..cur.min(max as usize));
        Ok(drain.collect())
    }

    pub fn write(&self, input: &[u8], line: bool) -> Result<u32> {
        let mut writer = self.shared.writer.lock();
        if line {
            writer.write_all(&self.sol)?;
            writer.write_all(input)?;
            writer.write_all(&self.eol)?;
            Ok((input.len() + self.sol.len() + self.eol.len()) as u32)
        } else {
            writer.write_all(input)?;
            Ok(input.len() as u32)
        }
    }

    pub fn readline(&self) -> Result<Vec<u8>> {
        let mut buffer = self.shared.buffer.lock();
        self.wait_eol(&mut buffer)?;
        self.get_line(buffer)
    }

    pub fn communicate(&self, input: &[u8]) -> Result<Vec<u8>> {
        let mut buffer = self.shared.buffer.lock();
        buffer.clear();
        self.write(input, true)?;
        self.wait_eol(&mut buffer)?;
        self.get_line(buffer)
    }

    fn wait_eol<T>(&self, lock: &mut MutexGuard<T>) -> Result<()> {
        if self.shared.seen_eol.wait_for(lock, self.timeout).timed_out() {
            Err(Error::comm_failed("no response"))
        } else {
            Ok(())
        }
    }
}

impl<W> Drop for CommClient<W> {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
    }
}

impl<R: Read + Send + 'static, W: Write + Send + 'static> CommThread<R, W> {
    pub fn spawn(mut connect: Connector<R, W>, sol: &[u8], eol: &[u8], timeout: Duration)
                 -> Result<CommClient<W>> {
        let (reader, writer) = connect()?;
        let shared = Arc::new(CommShared {
            writer: Mutex::new(writer),
            buffer: Mutex::new(Vec::with_capacity(2048)),
            seen_eol: Condvar::new(),
            connected: AtomicBool::new(true),
            stop: AtomicBool::new(false),
        });
        let mut comm = CommThread {
            connect,
            reader,
            eol: eol.into(),
            shared: Arc::clone(&shared),
        };
        thread::spawn(move || comm.thread());
        Ok(CommClient {
            shared,
            timeout,
            sol: sol.into(),
            eol: eol.into(),
        })
    }

    fn stopped(&self) -> bool {
        self.shared.stop.load(Ordering::Relaxed)
    }

    fn thread(&mut self) {
        debug!("reader thread started");
        let mut buf = [0; 1024];
        while !self.stopped() {
            while !self.stopped() {
                match self.reader.read(&mut buf) {
                    Ok(n) if n == 0 => {
                        warn!("reader connection closed");
                        break;
                    }
                    Ok(n) => {
                        let mut buffer = self.shared.buffer.lock();
                        debug!("reader got: {:?}", &buf[..n]);
                        buffer.extend(&buf[..n]);
                        if buffer.windows(self.eol.len()).any(|ch| ch == &self.eol[..]) {
                            self.shared.seen_eol.notify_one();
                        }
                    }
                    Err(e) => {
                        println!("reader error: {}", e);
                        break;
                    }
                }
            }
            self.shared.connected.store(false, Ordering::SeqCst);
            while !self.stopped() {
                match (self.connect)() {
                    Ok((reader, writer)) => {
                        self.reader = reader;
                        *self.shared.writer.lock() = writer;
                        self.shared.connected.store(true, Ordering::SeqCst);
                        break;
                    }
                    Err(e) => {
                        warn!("error during reconnect: {}", e);
                        thread::sleep(Duration::from_millis(100));
                    }
                }
            }
        }
        debug!("reader thread exited normally");
    }
}
