//! The accept loop and the per-connection gates that run before a router sees
//! anything.

use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{ensure, Result};

use super::access::AccessPolicy;
use super::connection::{fail, PEEK_TIMEOUT, WRITE_TIMEOUT};
use super::head::complete as complete_head;
use super::http::{self, RequestHead};
use super::limits::{Limits, Slot};
use super::shutdown::{Completion, ShutdownPending};
use super::unserved;

/// How long [`serve`] waits, once stopped, for in-flight connections to end.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(3);

/// What one server answers. [`serve`] calls it only for requests that already
/// passed the timeout, head and `Host` gates.
pub trait Router: Send + Sync + 'static {
    /// Take over a connection whose request head was only peeked, so its bytes
    /// are still unread: a WebSocket handshake, or any answer that must see
    /// the raw request. `None` means the router finished the connection;
    /// `Some(stream)` hands it back for ordinary HTTP handling.
    fn upgrade(
        &self,
        stream: TcpStream,
        peeked: &RequestHead,
        conn: &ConnInfo<'_>,
    ) -> Option<TcpStream>;

    /// Answer one consumed request head. `body_prefix` holds whatever body
    /// bytes arrived in the same reads as the head.
    fn handle(
        &self,
        stream: &mut TcpStream,
        head: &RequestHead,
        body_prefix: Vec<u8>,
        conn: &ConnInfo<'_>,
    ) -> io::Result<()>;
}

/// What a router knows about the connection it is answering.
pub struct ConnInfo<'a> {
    /// This connection's own accepted socket address: not necessarily the
    /// listener's bind address when that bind is a wildcard. It is what
    /// `policy` validates `Host` and `Origin` against.
    pub local: SocketAddr,
    /// The access policy resolved once for the whole server.
    pub policy: &'a AccessPolicy,
    /// The server's connection admission state.
    pub limits: &'a Arc<Limits>,
    /// Cleared when the server is stopping.
    pub running: &'a AtomicBool,
}

/// The fixed configuration of one [`serve`] call.
pub struct ServeConfig {
    /// Which requests pass `Host`, `Origin` and cookie checks.
    pub policy: AccessPolicy,
    /// Connection admission state, shared with the caller so it can observe
    /// occupancy.
    pub limits: Arc<Limits>,
    /// Name given to every connection thread.
    pub thread_name: &'static str,
}

/// Serve `listener` with `router` until `running` becomes false, then wait up
/// to three seconds for in-flight connections to finish.
///
/// Success means every handler has finished, including admitted mutations.
/// A drain deadline returns [`ShutdownPending`]; remaining handlers retain
/// the listener and router until they finish, even if the caller drops the
/// error. They may complete updates admitted before stopping. Routers using
/// [`http::read_body`] cancel incomplete bodies when stopping. A timeout never
/// reports completed shutdown.
///
/// Fails closed before accepting anything when the policy is remote (a
/// non-loopback bind) and carries no [`Auth`](super::auth::Auth).
pub fn serve(
    listener: TcpListener,
    router: Arc<dyn Router>,
    running: Arc<AtomicBool>,
    config: ServeConfig,
) -> Result<()> {
    let local = listener.local_addr()?;
    ensure!(
        !config.policy.is_remote() || config.policy.auth().is_some(),
        "refusing to serve {local}: a non-loopback listener requires an access token"
    );
    listener.set_nonblocking(true)?;
    let shared = Arc::new(Shared {
        listener,
        router,
        policy: config.policy,
        limits: config.limits,
        running,
        thread_name: config.thread_name,
        completion: Completion(Arc::new(AtomicBool::new(false))),
    });
    while shared.running.load(Ordering::SeqCst) {
        accept_connection(&shared.listener, &shared);
    }
    drain_connections(&shared, local)
}

/// Everything a connection thread needs, shared by one `Arc` across them.
pub(super) struct Shared {
    // Workers keep the listening address reserved until their handlers finish.
    // No worker accepts connections through this listener.
    listener: TcpListener,
    pub(super) router: Arc<dyn Router>,
    pub(super) policy: AccessPolicy,
    pub(super) limits: Arc<Limits>,
    pub(super) running: Arc<AtomicBool>,
    pub(super) thread_name: &'static str,
    // Must stay last: completion follows release of the listener and router.
    completion: Completion,
}

fn accept_connection(listener: &TcpListener, shared: &Arc<Shared>) {
    match listener.accept() {
        Ok((stream, _)) => unserved::spawn_connection(stream, shared),
        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
            thread::sleep(Duration::from_millis(50));
        }
        Err(error) => {
            tracing::warn!("dashboard accept failed: {error}");
            // A persistent failure (EMFILE) returns at once every time: without a pause this
            // loop spins and floods the log. It is shared by the long-lived planning server.
            thread::sleep(Duration::from_millis(50));
        }
    }
}

fn drain_connections(shared: &Arc<Shared>, local: SocketAddr) -> Result<()> {
    drain_until(shared, local, Instant::now() + DRAIN_TIMEOUT)
}

fn drain_until(shared: &Arc<Shared>, local: SocketAddr, deadline: Instant) -> Result<()> {
    while !drained(shared) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(50));
    }
    if !drained(shared) {
        return Err(ShutdownPending::new(local, Arc::clone(&shared.completion.0)).into());
    }
    Ok(())
}

fn drained(shared: &Arc<Shared>) -> bool {
    // A handler releases its slot just before its worker drops Shared. With
    // accepting stopped, no new owner can appear, so both observations must
    // agree before success can promise that the listening address is free.
    shared.limits.connection_count() == 0 && Arc::strong_count(shared) == 1
}

#[cfg(test)]
#[path = "serve_tests.rs"]
mod tests;

/// Handle one accepted connection, retaining ownership until the router returns.
///
/// `_slot` is the connection's reservation: holding it here releases it when
/// this thread ends, panic included. `Host` is checked ahead of the router, so
/// the gate covers upgrades, API routes and embedded assets alike.
pub(super) fn handle(mut stream: TcpStream, shared: &Shared, local: SocketAddr, _slot: Slot) {
    // A socket that refuses a timeout cannot be read or written safely (a
    // blocked syscall would pin this thread), so the connection ends here.
    if stream.set_read_timeout(Some(PEEK_TIMEOUT)).is_err()
        || stream.set_write_timeout(Some(WRITE_TIMEOUT)).is_err()
    {
        return;
    }
    let Some(peeked) = complete_head(&mut stream, &shared.running) else {
        return;
    };
    if !shared.policy.host_allowed(local, &peeked) {
        fail(&mut stream, 403, "Forbidden", b"host not allowed");
        return;
    }
    let conn = ConnInfo {
        local,
        policy: &shared.policy,
        limits: &shared.limits,
        running: &shared.running,
    };
    let Some(mut stream) = shared.router.upgrade(stream, &peeked, &conn) else {
        return;
    };
    let Ok((head, body_prefix)) = http::read_head(&mut stream) else {
        fail(&mut stream, 400, "Bad Request", b"bad request");
        return;
    };
    if let Err(error) = shared.router.handle(&mut stream, &head, body_prefix, &conn) {
        tracing::debug!("connection response failed: {error}");
    }
}
