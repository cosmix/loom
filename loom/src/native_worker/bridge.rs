//! In-boundary forwarder from loopback TCP to the proxy Unix socket, and the program launcher.
//!
//! Linux ([`run`]): the bridge binds its own loopback port inside the
//! boundary's network namespace. macOS ([`run_confined`]): the bridge runs
//! inside the Seatbelt profile, which grants no bind; it serves the listener
//! the supervisor bound before generating the profile and inherited to it,
//! and starts the program only when the supervisor releases it.

use std::ffi::OsString;
use std::io::ErrorKind;
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::os::fd::{FromRawFd, OwnedFd, RawFd};
use std::os::unix::net::UnixStream;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, ExitStatus, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};

use super::boundary::handshake;
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
    serve(listener, proxy_socket)
}

/// Starts a detached accept thread on `listener` forwarding to `proxy_socket`; the process
/// exit ends it. Returns the listener's port.
fn serve(listener: TcpListener, proxy_socket: &Path) -> Result<u16> {
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

/// The descriptors the supervisor hands a confined bridge.
#[derive(Debug, Clone, Copy)]
pub struct Inherited {
    /// The loopback listener the supervisor bound before generating the profile.
    pub listener: RawFd,
    /// The bridge's end of the control channel.
    pub control: RawFd,
}

/// The macOS bridge: confirms it runs inside the Seatbelt profile in its own session, reports
/// that on the control channel, waits for the supervisor's release, then serves the inherited
/// listener and runs `program` with the proxy environment for its port. Returns the program's
/// status, or fails once the control channel closes first, which closes the program's input.
pub fn run_confined(
    proxy_socket: &Path,
    inherited: Inherited,
    program: &Path,
    args: &[OsString],
) -> Result<ExitCode> {
    let (listener, mut control) = adopt(inherited)?;
    confirm_confined()?;
    close_on_exec_all().context("closing the supervisor's descriptors before codex starts")?;
    handshake::announce_ready(&mut control)?;
    handshake::await_go(&mut control)?;
    let port = serve(listener, proxy_socket)?;
    let mut child = Command::new(program)
        .args(args)
        .envs(proxy_environment(port))
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("starting {}", program.display()))?;
    let input = child.stdin.take().context("the program's input pipe")?;
    thread::Builder::new()
        .name("bridge-input".to_string())
        .spawn(move || copy_input(input))
        .context("starting the bridge input thread")?;
    supervise(child, control, program)
}

/// Copies the bridge's input to the program until either side closes.
fn copy_input(mut input: std::process::ChildStdin) {
    let _ = std::io::copy(&mut std::io::stdin().lock(), &mut input);
}

enum Ended {
    Exited(std::io::Result<ExitStatus>),
    ControlClosed,
}

/// Waits for `child` to exit or the supervisor's control channel to close, whichever is first.
fn supervise(
    mut child: std::process::Child,
    control: UnixStream,
    program: &Path,
) -> Result<ExitCode> {
    let (sender, receiver) = mpsc::channel();
    let exited = sender.clone();
    thread::Builder::new()
        .name("bridge-wait".to_string())
        .spawn(move || {
            let _ = exited.send(Ended::Exited(child.wait()));
        })
        .context("starting the bridge wait thread")?;
    thread::Builder::new()
        .name("bridge-control".to_string())
        .spawn(move || {
            handshake::wait_closed(control);
            let _ = sender.send(Ended::ControlClosed);
        })
        .context("starting the bridge control thread")?;
    match receiver.recv() {
        Ok(Ended::Exited(status)) => {
            let status = status.with_context(|| format!("waiting for {}", program.display()))?;
            Ok(exit_code(status))
        }
        Ok(Ended::ControlClosed) | Err(_) => {
            anyhow::bail!(
                "the supervisor closed the control channel; closing {}",
                program.display()
            )
        }
    }
}

/// Takes ownership of the inherited descriptors after checking that each is open and of the
/// expected kind: a loopback TCP listener and a Unix stream socket.
fn adopt(inherited: Inherited) -> Result<(TcpListener, UnixStream)> {
    let Inherited { listener, control } = inherited;
    anyhow::ensure!(
        listener > 2 && control > 2 && listener != control,
        "the inherited descriptors {listener} and {control} are not distinct non-standard descriptors"
    );
    for fd in [listener, control] {
        // SAFETY: `fcntl(F_GETFD)` only reads the descriptor flags of `fd`.
        if unsafe { libc::fcntl(fd, libc::F_GETFD) } == -1 {
            return Err(std::io::Error::last_os_error())
                .with_context(|| format!("the inherited descriptor {fd} is not open"));
        }
    }
    // SAFETY: both descriptors are open (checked above), distinct, and were handed to this
    // process by its supervisor for it alone; nothing else in this process owns them.
    let (listener, control) = unsafe {
        (
            OwnedFd::from_raw_fd(listener),
            OwnedFd::from_raw_fd(control),
        )
    };
    let listener = TcpListener::from(listener);
    let address = listener
        .local_addr()
        .context("the inherited listener is not a TCP socket")?;
    anyhow::ensure!(
        address.ip().is_loopback() && address.port() != 0,
        "the inherited listener is bound to {address}, not to a loopback port"
    );
    let control = UnixStream::from(control);
    control
        .peer_addr()
        .context("the inherited control channel is not a connected Unix socket")?;
    Ok((listener, control))
}

/// Refuses to start the program unless this process runs inside a Seatbelt sandbox and leads
/// its own session, as the supervisor's launch arranges.
#[cfg(target_os = "macos")]
fn confirm_confined() -> Result<()> {
    extern "C" {
        fn sandbox_check(
            pid: libc::pid_t,
            operation: *const libc::c_char,
            kind: libc::c_int,
            ...
        ) -> libc::c_int;
    }
    // SAFETY: `getpid` and `getsid(0)` take no pointers and cannot fail for this process.
    let (pid, session) = unsafe { (libc::getpid(), libc::getsid(0)) };
    anyhow::ensure!(session == pid, "the bridge does not lead its own session");
    // SAFETY: a null operation with filter type 0 (`SANDBOX_FILTER_NONE`) asks only whether
    // `pid` is sandboxed; no further arguments are read.
    let confined = unsafe { sandbox_check(pid, std::ptr::null(), 0) };
    anyhow::ensure!(
        confined == 1,
        "the bridge is not running inside a Seatbelt profile"
    );
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn confirm_confined() -> Result<()> {
    anyhow::bail!("the confined bridge needs macOS Seatbelt")
}

/// Marks every descriptor above the standard three close-on-exec, so the program inherits only
/// its standard streams. `/dev/fd` lists this process's open descriptors.
fn close_on_exec_all() -> Result<()> {
    let open: Vec<RawFd> = std::fs::read_dir("/dev/fd")
        .context("listing /dev/fd")?
        .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse().ok())
        .filter(|fd| *fd > 2)
        .collect();
    for fd in open {
        // SAFETY: `fcntl(F_SETFD)` only changes the descriptor flags of `fd`; one already closed
        // (the directory listing's own) fails with EBADF, which is ignored.
        unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) };
    }
    Ok(())
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
