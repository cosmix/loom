//! The control channel between the supervisor and the macOS bridge, a Unix
//! socket pair. The bridge sends [`READY`] once it confirmed it runs inside
//! the Seatbelt profile, then blocks until the supervisor, having recorded the
//! boundary's identity, sends [`GO`]; only then does it start Codex. Closing
//! the supervisor's end makes the bridge exit and close Codex's input.
//!
//! The protocol may start only through [`Control::activate`]: it refuses a
//! launch that confirmed nothing, so no `SandboxMode` exists for it and no
//! `thread/start` can be sent (`MachineConfig::sandbox` is required).
use std::io::{ErrorKind, Read, Write};
use std::os::unix::net::UnixStream;
#[cfg(any(target_os = "macos", test))]
use std::time::Duration;

use anyhow::{bail, Context, Result};

#[cfg(any(target_os = "macos", test))]
use crate::native_worker::protocol::wire::SandboxMode;

/// Sent by the bridge after it confirmed it runs confined.
pub(crate) const READY: &[u8; 6] = b"READY\n";
/// Sent by the supervisor once the identity is recorded.
pub(crate) const GO: &[u8; 3] = b"GO\n";

/// The supervisor's end of the control channel.
#[cfg(any(target_os = "macos", test))]
#[derive(Debug)]
pub(crate) struct Control {
    stream: UnixStream,
    confirmed: bool,
    released: bool,
}

#[cfg(any(target_os = "macos", test))]
impl Control {
    pub(crate) fn new(stream: UnixStream) -> Self {
        Self {
            stream,
            confirmed: false,
            released: false,
        }
    }

    /// Waits at most `timeout` for the bridge's [`READY`]. Anything else, an
    /// early close or the timeout is an error, and the launch stays
    /// unconfirmed.
    pub(crate) fn await_ready(&mut self, timeout: Duration) -> Result<()> {
        if let Err(error) = self.stream.set_read_timeout(Some(timeout)) {
            // macOS refuses the socket option once the peer has closed.
            if error.raw_os_error() == Some(libc::EINVAL) {
                bail!("the bridge exited before confirming its Seatbelt profile");
            }
            return Err(error).context("bound the bridge handshake");
        }
        let mut message = [0u8; READY.len()];
        match self.stream.read_exact(&mut message) {
            Ok(()) if &message == READY => {
                self.confirmed = true;
                Ok(())
            }
            Ok(()) => bail!("the bridge sent {message:?} instead of its ready message"),
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                bail!("the bridge did not confirm its Seatbelt profile within {timeout:?}")
            }
            Err(error) if error.kind() == ErrorKind::UnexpectedEof => {
                bail!("the bridge exited before confirming its Seatbelt profile")
            }
            Err(error) => Err(error).context("read the bridge's ready message"),
        }
    }

    /// Releases a confirmed bridge to start Codex, once. Refuses a launch
    /// whose bridge did not confirm the profile, and a second release.
    pub(crate) fn activate(&mut self) -> Result<SandboxMode> {
        if !self.confirmed {
            bail!("the boundary launch is unconfirmed: the bridge never reported its profile");
        }
        if self.released {
            bail!("the bridge was already released");
        }
        self.stream
            .write_all(GO)
            .context("release the bridge to start codex")?;
        self.released = true;
        Ok(SandboxMode::DangerFullAccess)
    }
}

/// The bridge's side: reports [`READY`] on `stream`.
pub(crate) fn announce_ready(stream: &mut UnixStream) -> Result<()> {
    stream
        .write_all(READY)
        .context("send the ready message to the supervisor")
}

/// The bridge's side: blocks until the supervisor sends [`GO`]. A close, or
/// anything else, refuses the start.
pub(crate) fn await_go(stream: &mut UnixStream) -> Result<()> {
    let mut message = [0u8; GO.len()];
    match stream.read_exact(&mut message) {
        Ok(()) if &message == GO => Ok(()),
        Ok(()) => bail!("the supervisor sent {message:?} instead of its release"),
        Err(error) if error.kind() == ErrorKind::UnexpectedEof => {
            bail!("the supervisor closed the control channel before releasing the bridge")
        }
        Err(error) => Err(error).context("read the supervisor's release"),
    }
}

/// The bridge's side: blocks until the supervisor's end is closed (or the
/// channel fails); anything sent after [`GO`] is ignored.
pub(crate) fn wait_closed(mut stream: UnixStream) {
    let mut buffer = [0u8; 64];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => return,
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(_) => return,
        }
    }
}

#[cfg(test)]
#[path = "handshake_tests.rs"]
mod tests;
