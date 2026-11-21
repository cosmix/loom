//! Allowlist CONNECT proxy served on a Unix socket for one native worker.

pub mod allow;
#[cfg(test)]
mod tests;
mod tunnel;

pub(crate) use tunnel::splice;

use std::collections::HashMap;
use std::fs::{self, Permissions};
use std::io::{self, ErrorKind};
use std::net::{Shutdown, TcpStream};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{bail, Context, Result};

use tunnel::Refusal;

/// `sun_path` holds 107 bytes plus the terminator; a margin keeps callers well clear of it.
const MAX_SOCKET_PATH_BYTES: usize = 100;
const SOCKET_DIR_MODE: u32 = 0o700;
const SOCKET_MODE: u32 = 0o600;
const ACCEPT_POLL: Duration = Duration::from_millis(50);

/// Resource bounds of one proxy.
#[derive(Debug, Clone, Copy)]
pub(super) struct Limits {
    pub max_connections: usize,
    pub head_timeout: Duration,
    pub connect_timeout: Duration,
    pub idle_timeout: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_connections: 32,
            head_timeout: Duration::from_secs(10),
            connect_timeout: Duration::from_secs(10),
            idle_timeout: Duration::from_secs(900),
        }
    }
}

/// Opens the upstream connection for an allowed `host` and port.
pub(super) type Connector = Arc<dyn Fn(&str, u16) -> io::Result<TcpStream> + Send + Sync>;

/// A running proxy. Dropping it stops the listener, closes every open tunnel, joins all of its
/// threads and removes the socket file.
pub struct ProxyHandle {
    shared: Arc<Shared>,
    accept: Option<JoinHandle<Vec<JoinHandle<()>>>>,
    socket: PathBuf,
}

struct Shared {
    allowed: Vec<String>,
    limits: Limits,
    connector: Connector,
    active: AtomicUsize,
    live: Mutex<HashMap<u64, UnixStream>>,
    stop: AtomicBool,
}

impl Shared {
    fn serve(&self, client: UnixStream) {
        let allowed: Vec<&str> = self.allowed.iter().map(String::as_str).collect();
        tunnel::serve(client, &allowed, &self.limits, &self.connector);
    }
}

/// Serves the proxy on `socket_path`, tunnelling only to `allowed` hosts on port 443 and only to
/// public addresses. The parent directory must be a 0700 directory owned by the current user.
pub fn start(socket_path: &Path, allowed: &[&str]) -> Result<ProxyHandle> {
    let limits = Limits::default();
    let timeout = limits.connect_timeout;
    let connector: Connector =
        Arc::new(move |host, port| tunnel::connect_public(host, port, timeout));
    start_with(socket_path, allowed, limits, connector)
}

pub(super) fn start_with(
    socket: &Path,
    allowed: &[&str],
    limits: Limits,
    connector: Connector,
) -> Result<ProxyHandle> {
    let listener = bind_listener(socket)?;
    let shared = Arc::new(Shared {
        allowed: allowed.iter().map(|e| e.to_ascii_lowercase()).collect(),
        limits,
        connector,
        active: AtomicUsize::new(0),
        live: Mutex::new(HashMap::new()),
        stop: AtomicBool::new(false),
    });
    let worker = Arc::clone(&shared);
    let spawned = thread::Builder::new()
        .name("proxy-accept".to_string())
        .spawn(move || accept_loop(&listener, &worker));
    match spawned {
        Ok(accept) => Ok(ProxyHandle {
            shared,
            accept: Some(accept),
            socket: socket.to_path_buf(),
        }),
        Err(e) => {
            let _ = fs::remove_file(socket);
            Err(e).context("starting the proxy accept thread")
        }
    }
}

fn bind_listener(socket: &Path) -> Result<UnixListener> {
    let len = socket.as_os_str().len();
    if len > MAX_SOCKET_PATH_BYTES {
        bail!(
            "proxy socket path {} is {len} bytes; at most {MAX_SOCKET_PATH_BYTES} are accepted \
             (the Unix socket sun_path limit is 107)",
            socket.display()
        );
    }
    let parent = socket
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .with_context(|| format!("proxy socket path {} has no parent", socket.display()))?;
    require_private_dir(parent)?;
    if fs::symlink_metadata(socket).is_ok() {
        bail!("proxy socket path {} already exists", socket.display());
    }
    let listener = UnixListener::bind(socket)
        .with_context(|| format!("binding proxy socket {}", socket.display()))?;
    let configured = fs::set_permissions(socket, Permissions::from_mode(SOCKET_MODE))
        .and_then(|()| listener.set_nonblocking(true));
    if let Err(e) = configured {
        let _ = fs::remove_file(socket);
        return Err(e).with_context(|| format!("configuring proxy socket {}", socket.display()));
    }
    Ok(listener)
}

/// The socket file name inside [`socket_dir`].
pub(super) const SOCKET_NAME: &str = "p.sock";

/// Creates a fresh directory for the proxy socket that passes `require_private_dir`.
/// tempfile creates directories with 0777 less the process umask, so a umask such as 002
/// would yield 0775; the mode is passed explicitly. The prefix and `SOCKET_NAME` stay short:
/// a confined command's `TMPDIR` can already take 85 bytes of the 100 allowed for the path.
pub(super) fn socket_dir() -> io::Result<tempfile::TempDir> {
    tempfile::Builder::new()
        .prefix("nw-")
        .permissions(Permissions::from_mode(SOCKET_DIR_MODE))
        .tempdir()
}

fn require_private_dir(dir: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(dir)
        .with_context(|| format!("reading proxy socket directory {}", dir.display()))?;
    if metadata.file_type().is_symlink() {
        bail!("proxy socket directory {} is a symlink", dir.display());
    }
    if !metadata.is_dir() {
        bail!(
            "proxy socket directory {} is not a directory",
            dir.display()
        );
    }
    // SAFETY: `getuid` has no preconditions and cannot fail.
    let uid = unsafe { libc::getuid() };
    if metadata.uid() != uid {
        bail!(
            "proxy socket directory {} is owned by uid {}, expected {uid}",
            dir.display(),
            metadata.uid()
        );
    }
    let mode = metadata.permissions().mode() & 0o777;
    if mode != SOCKET_DIR_MODE {
        bail!(
            "proxy socket directory {} has mode {mode:o}, expected {SOCKET_DIR_MODE:o}",
            dir.display()
        );
    }
    Ok(())
}

/// Runs until the stop flag is set; returns the handler threads still to be joined.
fn accept_loop(listener: &UnixListener, shared: &Arc<Shared>) -> Vec<JoinHandle<()>> {
    let mut handlers: Vec<JoinHandle<()>> = Vec::new();
    let mut next_id = 0u64;
    while !shared.stop.load(Ordering::SeqCst) {
        handlers.retain(|handle| !handle.is_finished());
        match listener.accept() {
            Ok((stream, _)) => {
                next_id += 1;
                handlers.extend(admit(stream, next_id, shared));
            }
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(_) => thread::sleep(ACCEPT_POLL),
        }
    }
    handlers
}

/// Answers 503 at the connection cap, before reading anything; otherwise registers the client
/// and starts its handler thread.
fn admit(mut stream: UnixStream, id: u64, shared: &Arc<Shared>) -> Option<JoinHandle<()>> {
    stream.set_nonblocking(false).ok()?;
    if shared.active.load(Ordering::SeqCst) >= shared.limits.max_connections {
        tunnel::refuse(
            &mut stream,
            Refusal::Unavailable,
            shared.limits.head_timeout,
        );
        return None;
    }
    let registration = Registration::new(shared, id, stream.try_clone().ok()?);
    let worker = Arc::clone(shared);
    thread::Builder::new()
        .name(format!("proxy-conn-{id}"))
        .spawn(move || {
            let _registration = registration;
            worker.serve(stream);
        })
        .ok()
}

/// Counts a live connection and keeps a clone of its client socket for shutdown; dropping it
/// (also while a handler unwinds) undoes both.
struct Registration {
    shared: Arc<Shared>,
    id: u64,
}

impl Registration {
    fn new(shared: &Arc<Shared>, id: u64, client: UnixStream) -> Self {
        shared.active.fetch_add(1, Ordering::SeqCst);
        lock(&shared.live).insert(id, client);
        Self {
            shared: Arc::clone(shared),
            id,
        }
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        lock(&self.shared.live).remove(&self.id);
        self.shared.active.fetch_sub(1, Ordering::SeqCst);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Drop for ProxyHandle {
    /// The accept thread is joined first so no client registers after the shutdown sweep; the
    /// sweep unblocks every handler, whose tunnel then closes its upstream.
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        let handlers = self
            .accept
            .take()
            .and_then(|accept| accept.join().ok())
            .unwrap_or_default();
        for client in lock(&self.shared.live).values() {
            let _ = client.shutdown(Shutdown::Both);
        }
        for handler in handlers {
            let _ = handler.join();
        }
        let _ = fs::remove_file(&self.socket);
    }
}
