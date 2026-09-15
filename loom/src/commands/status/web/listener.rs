//! Binding the dashboard's listening socket.
//!
//! The algorithm is unchanged from the loopback-only original: an explicit
//! port binds exactly (0 requests an ephemeral one from the OS), and an
//! omitted port walks forward from [`super::DEFAULT_PORT`] on `AddrInUse`
//! only. What is new is `host`: any bindable literal, not just `127.0.0.1`.
//! There is no port probing followed by a separate bind, no DNS resolution,
//! and no interface enumeration - `TcpListener::bind` is the only syscall
//! either function makes.

use std::io::ErrorKind;
use std::net::{IpAddr, SocketAddr, TcpListener};

use anyhow::{bail, Context, Result};

/// Bind an explicit port exactly on `host`, or select the first available
/// port from [`super::DEFAULT_PORT`].
pub(super) fn bind_listener(host: IpAddr, port: Option<u16>) -> Result<TcpListener> {
    match port {
        Some(port) => bind_exact(host, port),
        None => bind_first_available_port(host, super::DEFAULT_PORT),
    }
}

fn bind_exact(host: IpAddr, port: u16) -> Result<TcpListener> {
    let addr = SocketAddr::new(host, port);
    TcpListener::bind(addr).with_context(|| format!("failed to bind {addr}"))
}

/// Walk forward from `start_port` on `host` until a port binds or the range
/// is exhausted, advancing only past `AddrInUse` - any other bind error is
/// reported immediately rather than treated as one more occupied port.
pub(super) fn bind_first_available_port(host: IpAddr, start_port: u16) -> Result<TcpListener> {
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
