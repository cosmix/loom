//! In-boundary forwarder from loopback TCP to the proxy Unix socket, and the program launcher.

use std::ffi::OsString;
use std::io::ErrorKind;
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::os::unix::net::UnixStream;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, ExitStatus, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};

use super::proxy::splice;

/// Connections forwarded at once; further ones are dropped on accept.
const MAX_FORWARDS: usize = 64;
const ACCEPT_BACKOFF: Duration = Duration::from_millis(50);
const NO_PROXY_HOSTS: &str = "127.0.0.1,localhost,::1";

#[cfg(test)]
#[path = "bridge_tests.rs"]
mod tests;

/// Exposes `proxy_socket` on a loopback TCP port, runs `program` with the proxy environment
/// pointing at that port, and returns its exit status (`128 + signal` when it was killed).
pub fn run(proxy_socket: &Path, program: &Path, args: &[OsString]) -> Result<ExitCode> {
    let port = listen(proxy_socket)?;
    let mut child = Command::new(program)
        .args(args)
        .envs(proxy_environment(port))
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("starting {}", program.display()))?;
    let status = child
        .wait()
        .with_context(|| format!("waiting for {}", program.display()))?;
    Ok(exit_code(status))
}

/// The proxy variables set on the program, upper and lower case. `NO_PROXY` keeps loopback
/// services off the proxy. The URL carries no credentials: the Unix socket is the only gate.
pub(crate) fn proxy_environment(port: u16) -> Vec<(&'static str, String)> {
    let url = format!("http://127.0.0.1:{port}");
    let mut vars: Vec<(&'static str, String)> = [
        "HTTPS_PROXY",
        "https_proxy",
        "HTTP_PROXY",
        "http_proxy",
        "ALL_PROXY",
        "all_proxy",
    ]
    .into_iter()
    .map(|name| (name, url.clone()))
    .collect();
    vars.push(("NO_PROXY", NO_PROXY_HOSTS.to_string()));
    vars.push(("no_proxy", NO_PROXY_HOSTS.to_string()));
    vars
}

fn exit_code(status: ExitStatus) -> ExitCode {
    if let Some(code) = status.code() {
        // Exit statuses are 0..=255 on Unix; the cast keeps the low byte as the shell does.
        return ExitCode::from(code as u8);
    }
    match status.signal() {
        Some(signal) => ExitCode::from(u8::try_from(128 + signal).unwrap_or(u8::MAX)),
        None => ExitCode::FAILURE,
    }
}

/// Binds `127.0.0.1:0` and starts a detached accept thread forwarding to `proxy_socket`; the
/// process exit ends it. Returns the bound port.
fn listen(proxy_socket: &Path) -> Result<u16> {
    let listener =
        TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).context("binding the loopback proxy port")?;
    let port = listener
        .local_addr()
        .context("reading the loopback proxy port")?
        .port();
    let socket = proxy_socket.to_path_buf();
    thread::Builder::new()
        .name("bridge-accept".to_string())
        .spawn(move || accept_loop(&listener, &socket))
        .context("starting the bridge accept thread")?;
    Ok(port)
}

fn accept_loop(listener: &TcpListener, socket: &Path) {
    let active = Arc::new(AtomicUsize::new(0));
    loop {
        match listener.accept() {
            Ok((stream, _)) => forward(stream, socket, &active),
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(_) => thread::sleep(ACCEPT_BACKOFF),
        }
    }
}

fn forward(stream: TcpStream, socket: &Path, active: &Arc<AtomicUsize>) {
    if active.load(Ordering::SeqCst) >= MAX_FORWARDS {
        return;
    }
    let slot = Slot::new(active);
    let socket: PathBuf = socket.to_path_buf();
    let _ = thread::Builder::new()
        .name("bridge-forward".to_string())
        .spawn(move || {
            let _slot = slot;
            if let Ok(proxy) = UnixStream::connect(&socket) {
                splice(stream, proxy, None);
            }
        });
}

/// One counted forwarding connection; released on drop.
struct Slot(Arc<AtomicUsize>);

impl Slot {
    fn new(active: &Arc<AtomicUsize>) -> Self {
        active.fetch_add(1, Ordering::SeqCst);
        Self(Arc::clone(active))
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}
