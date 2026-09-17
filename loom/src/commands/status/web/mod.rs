//! `loom status --web`: an embedded dashboard with live WebSocket snapshots.
//!
//! # Access control
//!
//! The daemon authenticates its own clients with the `user.token` that
//! `status::ui::tui::daemon_client` presents. The dashboard holds that token
//! on the operator's behalf.
//!
//! By default this server binds `127.0.0.1` and serves `/api/status` and
//! `/ws` to any process on the host that can reach it - including other
//! local users - with only `Host` and `Origin` checks to keep a browser on
//! another site from reading it. That is the intended trade-off for an
//! operator-run localhost dashboard, but it is a deliberate downgrade of the
//! daemon's authentication model, not an oversight.
//!
//! `--host` widens that bind. Any bind that is not loopback - a concrete
//! remote address, or a wildcard (`0.0.0.0`/`::`) reachable from anywhere,
//! even when a particular client happens to arrive over loopback - runs
//! under `access::AccessPolicy`'s remote posture instead: every route
//! requires a cookie minted from a process token printed once at startup,
//! `Host` and `Origin` must name the connection's own accepted socket
//! address exactly, and HTTP carries the token and every snapshot
//! unencrypted. See `access` for the resolved policy and `auth` for the
//! cookie/token mechanics it shares with the terminal lane below.
//!
//! ## The write surface
//!
//! `/api/config` (`config_api`) is the one route that changes anything: it
//! reads and writes `~/.loom/config.toml` and `.loom/work/config.toml`. It
//! accepts the same reads as everything else here, and gates its writes
//! three deep:
//!
//! 1. **`Host`** - the DNS-rebinding (or, remotely, socket-identity) gate
//!    `connection::handle` already applies to every request ahead of
//!    routing.
//! 2. **`Origin`, strictly** - required rather than merely permitted, in
//!    both postures. Absence is fine for a same-origin `GET`, which carries
//!    no `Origin` at all, and is refused for a write, which always would.
//! 3. **A double-submit CSRF token** - minted once per server process, handed
//!    out only in the `GET /api/config` body, required in the `X-Loom-Csrf`
//!    header of every `POST`, and compared in constant time.
//!
//! The third gate holds only because this server sends no CORS header
//! anywhere: that is what stops a cross-site page reading the token out of the
//! `GET` or setting the header on a `POST`. Adding one would defeat it.
//! Request bodies are capped at `http::MAX_BODY_BYTES`, and no response
//! anywhere names an absolute path - a failure that would is logged and served
//! generically, because every local process can read this server by default.
//!
//! What the write surface does NOT do is raise the read posture above: a local
//! process that could already read the ledger can now also change loom's
//! configuration, which is a real widening of what a same-host caller can do
//! and the reason the gates above are not optional.
//!
//! ## Browser terminals
//!
//! Snapshot routes remain available exactly as before. The opt-in terminal
//! lane requires a startup token cookie and an `Origin` authority equal to the
//! request `Host`; anyone who can read the printed tokenized URL can type into
//! the agents. When enabled, this process adopts the live daemon's recorded
//! tmux socket directory once before serving; without a live daemon it keeps
//! the ambient `TMUX_TMPDIR`.
//!
//! The token cookie is named per port so two dashboards cannot clobber each
//! other's, but cookies are not port-scoped: any other page served from the
//! same host can still overwrite it for that shared host. That cannot forge a
//! valid token, but it does surface as terminals returning 401 ("dashboard
//! cookie required") until the operator re-opens the tokenized URL. A remote
//! dashboard's cookie alone never grants terminal capability: that still
//! derives only from `--terminals`.

mod access;
mod assets;
mod auth;
mod bootstrap;
mod broadcast;
mod config_api;
mod connection;
mod head;
mod http;
mod interfaces;
mod limits;
mod listener;
pub mod model;
mod terminal;
#[cfg(test)]
mod tests;
mod ws;

use std::net::{IpAddr, SocketAddr, TcpListener};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};

use crate::fs::tmux_tmpdir::{adopt_recorded_tmux_tmpdir, TmuxTmpdirAdoption};
use crate::fs::work_dir::WorkDir;

use access::AccessPolicy;

/// First port considered by `loom status --web` without a value.
pub const DEFAULT_PORT: u16 = 7373;

/// What `loom status --web` was started with.
#[derive(Debug, Clone, Default)]
pub struct ServeOptions {
    /// `Some(token)` when terminals are enabled.
    pub terminal_token: Option<String>,
    /// `Some(token)` when the bound listener is not loopback. Must equal
    /// `terminal_token` when both are set.
    pub dashboard_token: Option<String>,
}

/// The terminal lane configuration resolved from the listener's actual port.
#[derive(Clone)]
pub(super) struct TerminalLane {
    pub(super) token: String,
    pub(super) cookie_name: String,
}

#[cfg(test)]
pub(super) fn cookie_name_for_port(port: u16) -> String {
    TerminalLane::cookie_name(port)
}

/// Start the dashboard server until Ctrl-C.
pub fn execute(port: Option<u16>, terminals: bool, host: IpAddr) -> Result<()> {
    let work_dir = WorkDir::new(".")?;
    work_dir.load()?;
    let listener = listener::bind_listener(host, port)?;
    let local = listener.local_addr()?;
    let remote = !local.ip().is_loopback();
    let process_token = (remote || terminals).then(auth_token).transpose()?;
    let terminal_lane = terminals
        .then(|| {
            process_token
                .clone()
                .map(|token| TerminalLane::from_token(token, local.port()))
        })
        .flatten();
    if terminals {
        log_adoption(adopt_for(&work_dir));
    }
    print_startup(local, remote, terminals, process_token.as_deref());
    if assets::WEB_ASSETS.is_empty() {
        eprintln!(
            "warning: dashboard assets are not embedded in this binary; run `cd web && bun install && bun run build`, then rebuild loom"
        );
    }

    let running = Arc::new(AtomicBool::new(true));
    let on_ctrl_c = running.clone();
    ctrlc::set_handler(move || on_ctrl_c.store(false, Ordering::SeqCst))
        .context("failed to install Ctrl-C handler")?;
    serve(
        listener,
        PathBuf::from("."),
        running,
        ServeOptions {
            dashboard_token: remote.then(|| process_token.clone()).flatten(),
            terminal_token: terminal_lane.map(|lane| lane.token),
        },
    )
}

fn auth_token() -> Result<String> {
    terminal::token::mint().context("failed to mint a dashboard process token")
}

/// Print the URL an operator should open, and the remote-mode caveats.
fn print_startup(local: SocketAddr, remote: bool, terminals: bool, token: Option<&str>) {
    if !remote {
        match token {
            Some(token) if terminals => println!(
                "loom dashboard: http://{local}/?token={token}  (terminals enabled; Ctrl-C to stop)"
            ),
            _ => println!("loom dashboard: http://{local}/  (Ctrl-C to stop)"),
        }
        return;
    }
    if local.ip().is_unspecified() {
        print_startup_wildcard(local, terminals, token);
    } else {
        print_startup_concrete(local, terminals, token);
    }
}

/// A wildcard bind's own address names no reachable interface, so advertise
/// every configured address in the same IP family.
fn print_startup_wildcard(local: SocketAddr, terminals: bool, token: Option<&str>) {
    let token = token.unwrap_or_default();
    let note = if terminals { "; terminals enabled" } else { "" };
    println!("loom dashboard listening on {local} (remote access enabled{note})");
    for line in wildcard_bootstrap_lines(
        local.port(),
        token,
        interfaces::matching_addresses(local.ip()),
    ) {
        println!("{line}");
    }
    println!("  warning: this connection is plain HTTP; the token above grants dashboard and settings access to anyone who has it");
    println!("  (Ctrl-C to stop)");
}

fn wildcard_bootstrap_lines(
    port: u16,
    token: &str,
    addresses: impl IntoIterator<Item = IpAddr>,
) -> Vec<String> {
    addresses
        .into_iter()
        .map(|ip| {
            let location = if ip.is_loopback() {
                "this machine"
            } else {
                "another machine"
            };
            let endpoint = SocketAddr::new(ip, port);
            format!("  bootstrap from {location} at: http://{endpoint}/?token={token}")
        })
        .collect()
}

/// A concrete non-loopback bind's own address is directly reachable, and the
/// listener is not reachable on 127.0.0.1 - so the printed URL is the real one
/// rather than a placeholder plus a loopback fallback.
fn print_startup_concrete(local: SocketAddr, terminals: bool, token: Option<&str>) {
    let token = token.unwrap_or_default();
    let note = if terminals { "; terminals enabled" } else { "" };
    println!("loom dashboard listening on {local} (remote access enabled{note})");
    println!("  bootstrap from another machine at: http://{local}/?token={token}");
    println!("  warning: this connection is plain HTTP; the token above grants dashboard and settings access to anyone who has it");
    println!("  (Ctrl-C to stop)");
}

/// Adopt the live daemon's tmux directory for terminal attachments only.
pub(super) fn adopt_for(work_dir: &WorkDir) -> TmuxTmpdirAdoption {
    adopt_recorded_tmux_tmpdir(work_dir.root())
}

fn log_adoption(adoption: TmuxTmpdirAdoption) {
    if let TmuxTmpdirAdoption::Adopted { recorded, ambient } = adoption {
        let display = |value: &Option<std::ffi::OsString>| {
            value
                .as_ref()
                .map(|value| value.to_string_lossy().into_owned())
                .unwrap_or_else(|| "<unset>".to_owned())
        };
        println!(
            "Using the orchestrator's tmux socket dir (TMUX_TMPDIR={}) instead of this shell's ({})",
            display(&recorded), display(&ambient)
        );
    }
}

/// Serve an already-bound listener until `running` becomes false.
///
/// Fails closed rather than serving anything for a non-loopback (including
/// wildcard) `listener` that carries no valid process token in `options`:
/// this is the only gate a caller reaching this public function directly -
/// rather than through [`execute`] - gets, so it runs before the broadcaster
/// or any accept work starts.
///
/// `base` scopes the work directory the snapshots are read from, but *not* the
/// daemon connection: `daemon_client` resolves the socket and the `user.token`
/// through `commands::common::work_dir_path`, which is relative to the
/// process's current directory. [`execute`] passes `"."`, so the two agree
/// there; a caller passing some other directory - a test's tempdir, say - gets
/// file snapshots from `base` and any daemon subscription from the CWD.
pub fn serve(
    listener: TcpListener,
    base: PathBuf,
    running: Arc<AtomicBool>,
    options: ServeOptions,
) -> Result<()> {
    serve_with(listener, base, running, options, limits::Limits::new())
}

fn serve_with(
    listener: TcpListener,
    base: PathBuf,
    running: Arc<AtomicBool>,
    options: ServeOptions,
    limits: Arc<limits::Limits>,
) -> Result<()> {
    let policy = Arc::new(AccessPolicy::resolve(listener.local_addr()?, &options)?);
    listener.set_nonblocking(true)?;
    let lane = terminal_lane(&listener, &options)?;
    let broadcaster = broadcast::Broadcaster::spawn(base.clone(), running.clone(), lane.is_some());
    while running.load(Ordering::SeqCst) {
        accept_connection(
            &listener,
            &broadcaster,
            &base,
            &running,
            &limits,
            &lane,
            &policy,
        );
    }
    drain_connections(&limits);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn accept_connection(
    listener: &TcpListener,
    broadcaster: &broadcast::Broadcaster,
    base: &std::path::Path,
    running: &Arc<AtomicBool>,
    limits: &Arc<limits::Limits>,
    lane: &Option<TerminalLane>,
    policy: &Arc<AccessPolicy>,
) {
    match listener.accept() {
        Ok((stream, _)) => {
            spawn_connection(stream, broadcaster, base, running, limits, lane, policy)
        }
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
            thread::sleep(Duration::from_millis(50));
        }
        Err(error) => tracing::warn!("dashboard accept failed: {error}"),
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_connection(
    mut stream: std::net::TcpStream,
    broadcaster: &broadcast::Broadcaster,
    base: &std::path::Path,
    running: &Arc<AtomicBool>,
    limits: &Arc<limits::Limits>,
    lane: &Option<TerminalLane>,
    policy: &Arc<AccessPolicy>,
) {
    if let Err(error) = stream.set_nonblocking(false) {
        tracing::warn!("dashboard could not configure a client socket: {error}");
        return;
    }
    let Ok(local) = stream.local_addr() else {
        return;
    };
    let Some(slot) = limits::Slot::acquire(limits, limits::Lane::Connection) else {
        connection::reject_overloaded(&mut stream);
        return;
    };
    let broadcaster = broadcaster.clone();
    let base = base.to_path_buf();
    let running = running.clone();
    let limits = limits.clone();
    let lane = lane.clone();
    let policy = policy.clone();
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
    }
}

fn terminal_lane(listener: &TcpListener, options: &ServeOptions) -> Result<Option<TerminalLane>> {
    let port = listener.local_addr()?.port();
    Ok(options
        .terminal_token
        .as_ref()
        .map(|token| TerminalLane::from_token(token.clone(), port)))
}

fn drain_connections(limits: &limits::Limits) {
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while limits.connection_count() != 0 && std::time::Instant::now() < deadline {
        thread::sleep(Duration::from_millis(50));
    }
}
