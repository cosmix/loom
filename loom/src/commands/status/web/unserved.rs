//! Handing an accepted dashboard connection to its thread, and answering one
//! that no thread can take: a loaded server never closes a socket silently.

use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::thread;

use super::access::AccessPolicy;
use super::broadcast::Broadcaster;
use super::connection;
use super::limits::{Lane, Limits, Slot};
use super::TerminalLane;

#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_connection(
    mut stream: TcpStream,
    broadcaster: &Broadcaster,
    base: &Path,
    running: &Arc<AtomicBool>,
    limits: &Arc<Limits>,
    lane: &Option<TerminalLane>,
    policy: &Arc<AccessPolicy>,
) {
    let Some(local) = prepare_socket(&stream) else {
        return;
    };
    let Some(slot) = Slot::acquire(limits, Lane::Connection) else {
        connection::reject_unavailable(&mut stream, b"dashboard connection limit reached");
        return;
    };
    let broadcaster = broadcaster.clone();
    let base = base.to_path_buf();
    let running = running.clone();
    let limits = limits.clone();
    let lane = lane.clone();
    let policy = policy.clone();
    // The thread closure owns `stream` and is dropped when the spawn fails, so
    // the answer goes out on a clone made beforehand.
    let mut fallback = stream.try_clone().ok();
    if let Err(error) = thread::Builder::new()
        .name("loom-dashboard-conn".to_owned())
        .spawn(move || {
            connection::handle(
                stream,
                &broadcaster,
                &base,
                &running,
                &limits,
                lane.as_ref(),
                &policy,
                local,
                slot,
            )
        })
    {
        tracing::warn!("dashboard could not spawn a connection thread: {error}");
        if let Some(stream) = fallback.as_mut() {
            answer_unserved(stream);
        }
    }
}

/// Switch an accepted socket to blocking mode and return its local address;
/// `None` when either step fails, in which case the connection is dropped.
fn prepare_socket(stream: &TcpStream) -> Option<SocketAddr> {
    if let Err(error) = stream.set_nonblocking(false) {
        tracing::warn!("dashboard could not configure a client socket: {error}");
        return None;
    }
    stream.local_addr().ok()
}

/// Turn a connection away with a 503 because no thread could be spawned for
/// it. `connection::reject_unavailable` bounds the write and drains the unread
/// request bytes first, so the status arrives rather than an RST.
pub(super) fn answer_unserved(stream: &mut TcpStream) {
    connection::reject_unavailable(stream, b"dashboard could not serve this connection");
}

#[cfg(test)]
mod tests {
    use std::io::Read;
    use std::net::TcpListener;

    use super::*;
    use crate::commands::status::web::tests::skip_without_loopback;

    #[test]
    fn an_unserved_connection_is_answered_with_a_503() {
        if skip_without_loopback("an_unserved_connection_is_answered_with_a_503") {
            return;
        }
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).expect("connect");
        let (mut accepted, _) = listener.accept().expect("accept");

        answer_unserved(&mut accepted);
        drop(accepted);

        let mut response = String::new();
        client.read_to_string(&mut response).expect("read response");
        assert!(response.starts_with("HTTP/1.1 503"), "{response}");
    }
}
