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
//! under `web_server::access::AccessPolicy`'s remote posture instead:
//! viewing stays open to anyone who can reach the listener, `/api/config`
//! writes additionally require a cookie minted from a process token printed
//! once at startup, `Host` and `Origin` must name the connection's own
//! accepted socket address exactly, and HTTP carries the token and every
//! snapshot unencrypted. See `web_server::access` for the resolved policy and
//! `web_server::auth` for the cookie/token mechanics it shares with the
//! terminal lane below.
//!
//! ## The write surface
//!
//! `/api/config` (`config_api`) is the one route that changes anything: it
//! reads and writes `~/.loom/config.toml` and `.loom/work/config.toml`. It
//! accepts the same reads as everything else here, and gates its writes
//! three deep:
//!
//! 1. **`Host`** - the DNS-rebinding (or, remotely, socket-identity) gate
//!    `web_server::serve` already applies to every request ahead of
//!    routing.
//! 2. **`Origin`, strictly** - required rather than merely permitted, in
//!    both postures. Absence is fine for a same-origin `GET`, which carries
//!    no `Origin` at all, and is refused for a write, which always would.
//!    A remote bind also requires the dashboard cookie on every write.
//! 3. **A double-submit CSRF token** - minted once per server process, handed
//!    out only in the `GET /api/config` body, required in the `X-Loom-Csrf`
//!    header of every `POST`, and compared in constant time.
//!
//! The third gate holds only because this server sends no CORS header
//! anywhere: that is what stops a cross-site page reading the token out of the
//! `GET` or setting the header on a `POST`. Adding one would defeat it.
//! Request bodies are capped at `web_server::http::MAX_BODY_BYTES`, and no
//! response anywhere names an absolute path - a failure that would is logged
//! and served generically, because every local process can read this server
//! by default.
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
//! token required") until the operator re-opens the tokenized URL. A remote
//! dashboard's cookie alone never grants terminal capability: that still
//! derives only from `--terminals`.

mod broadcast;
mod config_api;
mod connection;
pub mod model;
mod router;
#[cfg(test)]
mod schema_parity;
mod startup;
mod terminal;
#[cfg(test)]
mod tests;

use std::net::{IpAddr, SocketAddr, TcpListener};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{bail, Context, Result};

use crate::fs::tmux_tmpdir::{adopt_recorded_tmux_tmpdir, TmuxTmpdirAdoption};
use crate::fs::work_dir::WorkDir;
use crate::web_server::access::AccessPolicy;
use crate::web_server::auth::{self, Auth};
use crate::web_server::limits::Limits;
use crate::web_server::{self, assets, listener, token, ServeConfig};

use router::DashboardRouter;
pub use startup::startup_lines;

/// First port considered by `loom status --web` without a value.
pub const DEFAULT_PORT: u16 = 7373;

/// Largest inbound message and frame accepted from a browser on either
/// WebSocket lane.
///
/// The dashboard snapshot socket discards every message the client sends, so
/// nothing there ever needs to buffer one; the terminal socket stages
/// keystrokes, which stay far under this bound. Both configure it as their
/// `max_message_size` and `max_frame_size` so tungstenite's 64 MiB / 16 MiB
/// defaults cannot let a local client make a connection thread hold that
/// much for nothing.
pub(super) const MAX_INBOUND_BYTES: usize = 64 * 1024;

/// Name of every dashboard connection thread.
const CONNECTION_THREAD: &str = "loom-dashboard-conn";

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
    let listener = bind_listener(host, port)?;
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
    startup::print_startup(local, remote, terminals, process_token.as_deref());
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

/// Bind an explicit port exactly on `host`, or the first available port from
/// [`DEFAULT_PORT`].
fn bind_listener(host: IpAddr, port: Option<u16>) -> Result<TcpListener> {
    listener::bind_listener(host, port, DEFAULT_PORT)
}

fn auth_token() -> Result<String> {
    token::mint().context("failed to mint a dashboard process token")
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
/// A bounded drain that still has active requests returns
/// [`web_server::ShutdownPending`]; those requests retain the listening address
/// and may finish previously admitted configuration updates. Only success
/// reports that every handler has finished.
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
    let limits = Limits::new(MAX_INBOUND_BYTES);
    serve_with(listener, base, running, options, limits)
}

fn serve_with(
    listener: TcpListener,
    base: PathBuf,
    running: Arc<AtomicBool>,
    options: ServeOptions,
    limits: Arc<Limits>,
) -> Result<()> {
    let local = listener.local_addr()?;
    let policy = AccessPolicy::resolve(local, &options)?;
    let lane = options
        .terminal_token
        .map(|token| TerminalLane::from_token(token, local.port()));
    let broadcaster = broadcast::Broadcaster::spawn(base.clone(), running.clone(), lane.is_some());
    let router = Arc::new(DashboardRouter::new(broadcaster, base, lane));
    let config = ServeConfig {
        policy,
        limits,
        thread_name: CONNECTION_THREAD,
    };
    web_server::serve(listener, router, running, config)
}

impl AccessPolicy {
    /// Resolve the dashboard's policy for a listener bound to `local_addr`,
    /// failing closed when that bind is not loopback and carries no valid
    /// process token - including when a caller reaches [`serve`] directly
    /// with a wildcard listener and no token at all. Only a remote bind
    /// authenticates with the dashboard cookie.
    pub(crate) fn resolve(local_addr: SocketAddr, options: &ServeOptions) -> Result<Self> {
        let remote = !local_addr.ip().is_loopback();
        let token = auth::resolve_process_token(
            options.dashboard_token.as_deref(),
            options.terminal_token.as_deref(),
        )?;
        if remote && token.is_none() {
            bail!(
                "refusing to serve {local_addr}: a non-loopback dashboard requires a process token"
            );
        }
        let auth = token
            .filter(|_| remote)
            .map(|token| Auth::new(token, token::cookie_name(local_addr.port())));
        Ok(Self::new(local_addr, auth))
    }
}
