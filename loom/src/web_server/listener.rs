//! Binding a server's listening socket.
//!
//! An explicit port binds exactly (0 requests an ephemeral one from the OS),
//! and an omitted port walks forward from the caller's default on `AddrInUse`
//! only. `host` is any bindable literal. There is no port probing followed by
//! a separate bind, no DNS resolution, and no interface enumeration -
//! `TcpListener::bind` is the only syscall either function makes.

use std::io::ErrorKind;
use std::net::{IpAddr, SocketAddr, TcpListener};

use anyhow::{bail, Context, Result};

/// Bind an explicit port exactly on `host`, or select the first available
/// port from `default_port`.
pub fn bind_listener(host: IpAddr, port: Option<u16>, default_port: u16) -> Result<TcpListener> {
    match port {
        Some(port) => bind_exact(host, port),
        None => bind_first_available_port(host, default_port),
    }
}

fn bind_exact(host: IpAddr, port: u16) -> Result<TcpListener> {
    let addr = SocketAddr::new(host, port);
    TcpListener::bind(addr).with_context(|| format!("failed to bind {addr}"))
}

/// Walk forward from `start_port` on `host` until a port binds or the range
/// is exhausted, advancing only past `AddrInUse` - any other bind error is
/// reported immediately rather than treated as one more occupied port.
pub fn bind_first_available_port(host: IpAddr, start_port: u16) -> Result<TcpListener> {
    for port in start_port..=u16::MAX {
        let addr = SocketAddr::new(host, port);
        match TcpListener::bind(addr) {
            Ok(listener) => return Ok(listener),
            Err(error) if error.kind() == ErrorKind::AddrInUse => continue,
            Err(error) => return Err(error).with_context(|| format!("failed to bind {addr}")),
        }
    }
    bail!("failed to bind a port on {host} from {start_port} through 65535");
}

/// Rebind `local`, polling to a deadline on `AddrInUse`: proof that its listener closed.
///
/// Dropping the last owner closes the listener, but a child another test thread is spawning
/// in that instant holds a copy of the descriptor between fork and exec (std opens it
/// `SOCK_CLOEXEC`, so exec drops it). Until then the address stays bound and a connect to it
/// succeeds. A listener still owned in this process stays `AddrInUse` past the deadline and
/// fails the caller. The returned listener holds the address, so no later bind can mask it.
#[cfg(test)]
pub(crate) fn rebind_when_released(local: SocketAddr) -> std::io::Result<TcpListener> {
    use std::time::{Duration, Instant};

    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        match TcpListener::bind(local) {
            Err(error) if error.kind() == ErrorKind::AddrInUse && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            other => return other,
        }
    }
}
