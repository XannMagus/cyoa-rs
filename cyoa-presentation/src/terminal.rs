//! Stoppable Linux input, without a permanently blocked stdin thread.
use crate::headless::{Input, InputEvent};
use std::{io, time::Duration};

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use rustix::{
        event::{PollFd, PollFlags, poll},
        fs::{OFlags, fcntl_getfl, fcntl_setfl},
    };
    use std::{
        os::fd::AsFd,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
    };

    /// Restore descriptor flags even when setup or output fails. Nonblocking
    /// output reports backpressure as an I/O error rather than freezing cancel.
    pub struct Flags<F: AsFd> {
        fd: F,
        original: OFlags,
    }
    impl<F: AsFd> Flags<F> {
        pub fn new(fd: F) -> io::Result<Self> {
            let original = fcntl_getfl(&fd)?;
            fcntl_setfl(&fd, original | OFlags::NONBLOCK)?;
            Ok(Self { fd, original })
        }
        pub fn get_mut(&mut self) -> &mut F {
            &mut self.fd
        }
    }
    impl<F: AsFd> Drop for Flags<F> {
        fn drop(&mut self) {
            let _ = fcntl_setfl(&self.fd, self.original);
        }
    }
    pub struct InterruptFlag {
        flag: Arc<AtomicBool>,
        registration: signal_hook::SigId,
    }
    impl InterruptFlag {
        pub fn new() -> io::Result<Self> {
            let flag = Arc::new(AtomicBool::new(false));
            let registration =
                signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&flag))?;
            Ok(Self { flag, registration })
        }
        pub fn take(&self) -> bool {
            self.flag.swap(false, Ordering::SeqCst)
        }
    }
    impl Drop for InterruptFlag {
        fn drop(&mut self) {
            signal_hook::low_level::unregister(self.registration);
        }
    }
    pub struct TerminalInput {
        stdin: Flags<io::Stdin>,
        bytes: Vec<u8>,
        eof: bool,
        interrupt: InterruptFlag,
    }
    impl TerminalInput {
        pub fn new() -> io::Result<Self> {
            let stdin = Flags::new(io::stdin())?;
            let interrupt = InterruptFlag::new()?;
            Ok(Self {
                stdin,
                bytes: vec![],
                eof: false,
                interrupt,
            })
        }
        fn line(&mut self, end: usize, newline: bool) -> io::Result<InputEvent> {
            let bytes: Vec<_> = self.bytes.drain(..end + usize::from(newline)).collect();
            let mut text = String::from_utf8(bytes[..end].to_vec())
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            if text.ends_with('\r') {
                text.pop();
            }
            Ok(InputEvent::Line(text))
        }
    }
    impl Input for TerminalInput {
        fn poll(&mut self, timeout: Duration) -> io::Result<InputEvent> {
            if self.interrupt.take() {
                return Ok(InputEvent::Interrupt);
            }
            if let Some(end) = self.bytes.iter().position(|b| *b == b'\n') {
                return self.line(end, true);
            }
            if self.eof {
                if !self.bytes.is_empty() {
                    return self.line(self.bytes.len(), false);
                }
                return Ok(InputEvent::Eof);
            }
            let mut fds = [PollFd::new(&self.stdin.fd, PollFlags::IN)];
            let milliseconds = timeout.as_millis().min(1000) as i64;
            let timespec = rustix::event::Timespec {
                tv_sec: milliseconds / 1000,
                tv_nsec: (milliseconds % 1000) * 1_000_000,
            };
            match poll(&mut fds, Some(&timespec)) {
                Err(rustix::io::Errno::INTR) => return Ok(InputEvent::Pending),
                Err(error) => return Err(error.into()),
                Ok(0) => return Ok(InputEvent::Pending),
                Ok(_) => (),
            }
            // Drop poll's borrowed descriptor before reading.
            let mut chunk = [0u8; 1024];
            match rustix::io::read(&self.stdin.fd, &mut chunk) {
                Ok(0) => self.eof = true,
                Ok(n) => self.bytes.extend_from_slice(&chunk[..n]),
                Err(rustix::io::Errno::AGAIN | rustix::io::Errno::INTR) => {
                    return Ok(InputEvent::Pending);
                }
                Err(e) => return Err(e.into()),
            }
            if self.bytes.len() > 65_536 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "input line exceeds 64 KiB",
                ));
            }
            if let Some(end) = self.bytes.iter().position(|b| *b == b'\n') {
                return self.line(end, true);
            }
            if self.eof && self.bytes.is_empty() {
                Ok(InputEvent::Eof)
            } else {
                Ok(InputEvent::Pending)
            }
        }
    }
}
#[cfg(target_os = "linux")]
pub use linux::{Flags, InterruptFlag, TerminalInput};

/// Keep unsupported hosts explicit rather than installing a blocking reader.
#[cfg(not(target_os = "linux"))]
pub fn unsupported() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "headless terminal I/O currently supports Linux only",
    )
}
