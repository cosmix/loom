//! Planning server: the local web app behind bare `loom plan`.
//!
//! `launch` binds, builds the [`context::PlanContext`] and starts the accept loop, then leaves
//! the rest to the `loading` thread: the prerequisite checks, the `doc/` watcher's baseline, the
//! project, every route module's `install`, the watcher and the 1 s tick. [`run()`] is the
//! command: project root, runtime directory, lock, `launch`, startup lines, browser, signals; none
//! of it waits for the load. [`start`] waits for it.

pub mod authoring;
mod background;
pub mod browser;
pub mod chat;
pub mod concept;
pub mod context;
pub mod extensions;
pub mod features;
pub mod files;
pub mod gate;
pub mod guidance;
pub mod ids;
mod launch;
pub mod lifecycle;
mod loading;
pub mod lock;
pub mod prereq;
pub mod presence;
pub mod pressure;
pub mod request;
pub mod review;
pub mod root;
pub mod router;
pub mod routes;
pub mod run;
pub mod search;
pub mod server_info;
pub mod shutdown;
pub mod startup;
pub mod summaries;
pub mod watch;
pub mod wire;
mod worker_owner;
pub mod ws;

#[cfg(test)]
mod authoring_tests;
#[cfg(test)]
mod browser_tests;
#[cfg(test)]
mod context_contract_tests;
#[cfg(test)]
mod gate_tests;
#[cfg(test)]
mod ids_tests;
#[cfg(test)]
mod lifecycle_tests;
#[cfg(test)]
mod lock_tests;
#[cfg(test)]
mod prereq_tests;
#[cfg(test)]
mod presence_tests;
#[cfg(test)]
mod root_tests;
#[cfg(test)]
mod shutdown_tests;
#[cfg(test)]
mod startup_tests;
#[cfg(test)]
mod tests;

use std::fmt;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};

use crate::cli::PlanServeArgs;
use crate::planning::runtime::{create_runtime_dir, server_runtime_paths};
use browser::{EnvView, OpenDecision};
use context::{display_addrs, PlanContext};
use launch::{start_owned, Launch};
use loading::{LoadState, LOAD_WAIT};
use lock::Acquired;
use prereq::ReadOnly;
use routes::RouteSpec;
use wire::message::PlanMessage;

/// Port the planning server binds when `--port` is not given.
pub const DEFAULT_PORT: u16 = 7272;

/// What [`start`] serves.
pub struct ServerOptions {
    /// The project root (the directory holding `doc/`).
    pub root: PathBuf,
    pub host: IpAddr,
    /// `None` walks forward from [`DEFAULT_PORT`]; `Some(0)` takes an OS-chosen port.
    pub port: Option<u16>,
    pub read_only: ReadOnly,
}

/// A started server. Dropping it leaves the threads running; call [`RunningServer::stop`].
pub struct RunningServer {
    /// The bound address; its port is the one in every URL.
    pub local: SocketAddr,
    /// The access token (`GET /?token=<token>` bootstraps the cookies).
    pub token: String,
    pub ctx: Arc<PlanContext>,
    running: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    workers: worker_owner::Workers,
    ownership: Arc<lock::Ownership>,
}

impl RunningServer {
    /// Stop accepting and ask the watcher and tick to end. Wait up to three
    /// seconds for their owner to reap them and drain every request; otherwise
    /// return [`crate::web_server::ShutdownPending`]. The cleanup owner retains all
    /// handles, the listener reservation and singleton through context disposal.
    /// Context references cloned by callers remain their own responsibility.
    pub fn stop(self) -> Result<()> {
        self.running.store(false, Ordering::SeqCst);
        self.stop.store(true, Ordering::SeqCst);
        self.ctx.shutdown().mark_stopping();
        // The cleanup owner still holds these; their last-drop work belongs
        // there, so child disposal cannot block this caller after its deadline.
        drop(self.ctx);
        drop(self.ownership);
        self.workers.stop()
    }

    /// Wait for the shutdown drains `drains` joins (from [`begin_shutdown`]), then stop.
    pub fn stop_when_drained(self, drains: JoinHandle<()>) -> Result<()> {
        if drains.join().is_err() {
            tracing::warn!("the loom plan shutdown drain panicked");
        }
        self.stop()
    }

    fn is_serving(&self) -> bool {
        self.workers.is_serving()
    }

    /// Wait for the background load; a load that failed or never ended stops the server.
    fn loaded(self) -> Result<Self> {
        let error = match self.ctx.loading().wait(LOAD_WAIT, || false) {
            LoadState::Ready => return Ok(self),
            LoadState::Failed(message) => anyhow!(message),
            LoadState::Loading => anyhow!("the project did not load within {LOAD_WAIT:?}"),
        };
        // The load's own error names the cause; a failed load always makes the stop report one.
        let _ = self.stop();
        Err(error)
    }
}

/// Bind, mint the token and start the accept loop and the background load, and wait for the load.
/// Touches neither the lock nor the runtime directory.
pub fn start(opts: ServerOptions) -> Result<RunningServer> {
    start_with_routes(opts, routes::all_routes())
}

/// [`start`] with an explicit route table; tests serve routes of their own with it.
fn start_with_routes(opts: ServerOptions, routes: Vec<RouteSpec>) -> Result<RunningServer> {
    start_owned(Launch {
        root: opts.root,
        host: opts.host,
        port: opts.port,
        read_only: Box::new(move || opts.read_only),
        install: Box::new(routes::install_all),
        routes,
        ownership: Arc::default(),
    })?
    .loaded()
}

/// `loom plan` refused to start because a server already runs for this project. The lines
/// naming it are already on stderr; the command exits 1 without printing this error again.
#[derive(Debug)]
pub struct AlreadyRunning;

impl fmt::Display for AlreadyRunning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("loom plan is already running for this project")
    }
}

impl std::error::Error for AlreadyRunning {}

/// Bare `loom plan`: serve this project until stopped.
pub fn run(args: &PlanServeArgs) -> Result<()> {
    let cwd = std::env::current_dir().context("cannot read the current directory")?;
    let root = root::resolve_project(&cwd)?;
    let (paths, ignored) = server_runtime_paths(&root)?;
    if ignored.is_some() {
        eprintln!("warning: LOOM_PLAN_STATE is set in this shell and is ignored by the server");
    }
    create_runtime_dir(&paths)?;
    let lock = match lock::acquire(&paths.lock)? {
        Acquired::Held(lock) => lock,
        Acquired::Running(record) => {
            for line in startup::already_running_lines(&record) {
                eprintln!("{line}");
            }
            return Err(AlreadyRunning.into());
        }
    };
    let ownership = Arc::new(lock::Ownership::default());
    ownership.retain(lock);
    let server = launch_command(&root, args, ownership)?;
    // Before the URL exists: a SIGINT right after it must drain, not kill the server.
    let drains_rx = match install_stop_handler(&server) {
        Ok(drains_rx) => drains_rx,
        Err(error) => {
            let _ = server.stop();
            return Err(error);
        }
    };
    let base = display_addrs(server.local).first().map_or_else(
        || format!("http://{}", server.local),
        |addr| format!("http://{addr}"),
    );
    let published = server.ownership.publish(&base, &server.token);
    if let Err(error) = published {
        server
            .stop()
            .with_context(|| format!("cannot publish server startup: {error}"))?;
        return Err(error);
    }
    announce(&server, &root, args.no_open);
    serve_until_stopped(server, drains_rx)
}

/// Start the command's server, whose load prints the read-only lines once the checks have run.
fn launch_command(
    root: &Path,
    args: &PlanServeArgs,
    ownership: Arc<lock::Ownership>,
) -> Result<RunningServer> {
    start_owned(Launch {
        root: root.to_path_buf(),
        host: args.host,
        port: args.port,
        read_only: Box::new(ReadOnly::detect),
        install: Box::new(|ctx| {
            for line in startup::read_only_lines(ctx.read_only().problems()) {
                println!("{line}");
            }
            routes::install_all(ctx);
        }),
        routes: routes::all_routes(),
        ownership,
    })
}

/// Print the startup lines, then open the first URL unless `--no-open`, SSH or no display. The
/// read-only lines follow from the load, when the checks have run.
fn announce(server: &RunningServer, root: &Path, no_open: bool) {
    let env = EnvView::current();
    let bind = server.ctx.bind();
    let lines = startup::startup_lines(&startup::Startup {
        root,
        urls: bind.urls.clone(),
        port: server.local.port(),
        remote: bind.remote,
        ssh: env.ssh_connection || env.ssh_tty,
        hostname: hostname(),
        read_only: &[],
    });
    for line in lines {
        println!("{line}");
    }
    if let (OpenDecision::Open, Some(url)) = (browser::decide(&env, no_open), bind.urls.first()) {
        browser::open(url);
    }
}

/// Install the Ctrl-C handler; the first signal sends its drains down the returned channel. A
/// second signal exits at once.
fn install_stop_handler(server: &RunningServer) -> Result<mpsc::Receiver<JoinHandle<()>>> {
    let (drains_tx, drains_rx) = mpsc::channel();
    let ctx = Arc::clone(&server.ctx);
    let on_first = move || {
        let _ = drains_tx.send(begin_shutdown(&ctx));
    };
    shutdown::install_signal_handler(
        server.ctx.shutdown_handle(),
        Box::new(on_first),
        std::process::exit,
    )
    .context("failed to install the Ctrl-C handler")?;
    Ok(drains_rx)
}

/// Block until the first stop signal's drains finish (then return the stop result), or until the
/// accept loop ends by itself (then return its result).
fn serve_until_stopped(
    server: RunningServer,
    drains_rx: mpsc::Receiver<JoinHandle<()>>,
) -> Result<()> {
    loop {
        match drains_rx.recv_timeout(Duration::from_millis(200)) {
            Ok(drains) => return server.stop_when_drained(drains),
            Err(RecvTimeoutError::Timeout) if server.is_serving() => {}
            Err(_) => return server.stop(),
        }
    }
}

/// The first stop signal: tell every tab, then drain every registered shutdown hook while the
/// accept loop, every WebSocket and every route stay up. The handle joins when the last drain
/// returns.
pub fn begin_shutdown(ctx: &PlanContext) -> JoinHandle<()> {
    lifecycle::line("stopping; press Ctrl-C again to exit at once");
    // Begin first: it sets the stopping flag, so a request racing the broadcast already sees
    // the server as stopping.
    let drains = ctx.shutdown().begin(lifecycle::line);
    ctx.hub().broadcast(&PlanMessage::ServerStopping);
    drains
}

/// This machine's host name, for the SSH tunnel hint; `localhost` when it cannot be read.
pub fn hostname() -> String {
    let mut buffer = [0_u8; 256];
    // SAFETY: `buffer` is valid for writes of `buffer.len()` bytes, the length passed;
    // gethostname writes at most that many bytes into it.
    let status = unsafe { libc::gethostname(buffer.as_mut_ptr().cast(), buffer.len()) };
    if status != 0 {
        return "localhost".to_owned();
    }
    let end = buffer
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(buffer.len());
    String::from_utf8_lossy(&buffer[..end]).into_owned()
}
