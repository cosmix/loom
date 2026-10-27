//! Planning server: the local web app behind bare `loom plan`.
//!
//! [`start`] binds, takes the `doc/` watcher's baseline, builds the [`context::PlanContext`],
//! runs every route module's `install`, then starts the watcher, the 1 s tick and the accept
//! loop. [`run`] is the command: project root, runtime directory, lock, [`start`], startup
//! lines, browser, signals.

pub mod authoring;
pub mod browser;
pub mod context;
pub mod extensions;
pub mod files;
pub mod gate;
pub mod ids;
pub mod lifecycle;
pub mod lock;
pub mod prereq;
pub mod presence;
pub mod request;
pub mod root;
pub mod router;
pub mod routes;
pub mod search;
pub mod server_info;
pub mod shutdown;
pub mod startup;
pub mod watch;
pub mod wire;
pub mod ws;

#[cfg(test)]
mod authoring_tests;
#[cfg(test)]
mod browser_tests;
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
use std::fs::{File, Permissions};
use std::net::{IpAddr, SocketAddr, TcpListener};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};

use crate::cli::PlanServeArgs;
use crate::fs::safe_fs::{safe_create_dir_all, safe_open_dirfd};
use crate::planning::runtime::{server_runtime_paths, RuntimePaths};
use crate::planning::watch::{OwnWrites, Watcher};
use crate::web_server::access::AccessPolicy;
use crate::web_server::auth::Auth;
use crate::web_server::limits::Limits;
use crate::web_server::listener::bind_listener;
use crate::web_server::{self, token, ServeConfig};
use browser::{EnvView, OpenDecision};
use context::{display_addrs, BindInfo, PlanContext};
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
    serve: JoinHandle<Result<()>>,
    workers: Vec<JoinHandle<()>>,
}

impl RunningServer {
    /// Stop accepting, end every connection, the watcher and the tick, and wait for them.
    pub fn stop(self) -> Result<()> {
        self.running.store(false, Ordering::SeqCst);
        self.stop.store(true, Ordering::SeqCst);
        let served = self
            .serve
            .join()
            .map_err(|_| anyhow!("the loom plan accept loop panicked"))?;
        for worker in self.workers {
            let _ = worker.join();
        }
        served
    }

    /// Wait for the shutdown drains `drains` joins (from [`begin_shutdown`]), then stop.
    pub fn stop_when_drained(self, drains: JoinHandle<()>) -> Result<()> {
        if drains.join().is_err() {
            tracing::warn!("the loom plan shutdown drain panicked");
        }
        self.stop()
    }

    fn is_serving(&self) -> bool {
        !self.serve.is_finished()
    }
}

/// Bind, mint the token, take the watcher's baseline, load the project, build the context and
/// run every module's `install`, then start the watcher, the tick and the accept loop. Touches neither the lock
/// nor the runtime directory.
pub fn start(opts: ServerOptions) -> Result<RunningServer> {
    start_with_routes(opts, routes::all_routes())
}

/// [`start`] with an explicit route table; tests serve routes of their own with it.
fn start_with_routes(opts: ServerOptions, routes: Vec<RouteSpec>) -> Result<RunningServer> {
    let listener = bind_listener(opts.host, opts.port, DEFAULT_PORT)?;
    let local = listener.local_addr()?;
    let token = token::mint().context("failed to mint the loom plan access token")?;
    let bind = BindInfo::new(local, &token);
    let (ctx, watcher) = load(opts.root, opts.read_only, bind)?;
    let running = Arc::new(AtomicBool::new(true));
    let stop = Arc::new(AtomicBool::new(false));
    let started = spawn_workers(&ctx, watcher, &stop).and_then(|workers| {
        let serve = spawn_accept_loop(listener, &ctx, routes, &token, &running)?;
        Ok((serve, workers))
    });
    let (serve, workers) = started.inspect_err(|_| stop.store(true, Ordering::SeqCst))?;
    Ok(RunningServer {
        local,
        token,
        ctx,
        running,
        stop,
        serve,
        workers,
    })
}

/// Take the watcher's baseline, then load the project into a new context and run every route
/// module's `install`. The baseline comes first, so a document edited while the project and
/// the stores load reaches them as the watcher's first batch.
fn load(root: PathBuf, read_only: ReadOnly, bind: BindInfo) -> Result<(Arc<PlanContext>, Watcher)> {
    let own_writes = OwnWrites::default();
    let watcher = watch::baseline(root.clone(), own_writes.clone());
    let ctx = Arc::new(PlanContext::new(root, read_only, bind, own_writes)?);
    routes::install_all(&ctx);
    Ok((ctx, watcher))
}

fn spawn_workers(
    ctx: &Arc<PlanContext>,
    watcher: Watcher,
    stop: &Arc<AtomicBool>,
) -> Result<Vec<JoinHandle<()>>> {
    let (watcher, errors) = watch::spawn_watcher(Arc::clone(ctx), watcher, Arc::clone(stop))?;
    let tick = watch::spawn_tick(Arc::clone(ctx), errors, Arc::clone(stop));
    let tick = tick.inspect_err(|_| stop.store(true, Ordering::SeqCst))?;
    Ok(vec![watcher, tick])
}

fn spawn_accept_loop(
    listener: TcpListener,
    ctx: &Arc<PlanContext>,
    routes: Vec<RouteSpec>,
    token: &str,
    running: &Arc<AtomicBool>,
) -> Result<JoinHandle<Result<()>>> {
    let local = listener.local_addr()?;
    let auth = Auth::new(token.to_owned(), gate::access_cookie_name(local.port()));
    let config = ServeConfig {
        policy: AccessPolicy::new(local, Some(auth)),
        limits: Limits::new(ws::MAX_INBOUND_BYTES),
        thread_name: "loom-plan-conn",
    };
    let router = Arc::new(router::PlanRouter::new(Arc::clone(ctx), routes));
    let running = Arc::clone(running);
    thread::Builder::new()
        .name("loom-plan-accept".into())
        .spawn(move || web_server::serve(listener, router, running, config))
        .context("cannot start the loom plan accept loop")
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
    let mut lock = match lock::acquire(&paths.lock)? {
        Acquired::Held(lock) => lock,
        Acquired::Running(record) => {
            for line in startup::already_running_lines(&record) {
                eprintln!("{line}");
            }
            return Err(AlreadyRunning.into());
        }
    };
    let options = ServerOptions {
        root: root.clone(),
        host: args.host,
        port: args.port,
        read_only: ReadOnly::detect(),
    };
    let server = start(options)?;
    let base = display_addrs(server.local).first().map_or_else(
        || format!("http://{}", server.local),
        |addr| format!("http://{addr}"),
    );
    if let Err(error) = lock.publish(&base, &server.token) {
        let _ = server.stop();
        return Err(error);
    }
    announce(&server, &root, args.no_open);
    serve_until_stopped(server)
}

/// Create the runtime directory mode 0700, refusing a symlink or a non-directory anywhere
/// below the loom user directory.
fn create_runtime_dir(paths: &RuntimePaths) -> Result<()> {
    let base = paths
        .planning_root
        .parent()
        .context("the planning root has no parent directory")?;
    std::fs::create_dir_all(base).with_context(|| format!("cannot create {}", base.display()))?;
    let rel = paths
        .dir
        .strip_prefix(base)
        .context("the runtime directory is outside the loom user directory")?;
    safe_create_dir_all(base, rel, 0o700)?;
    let dir = File::from(safe_open_dirfd(&paths.dir)?);
    dir.set_permissions(Permissions::from_mode(0o700))
        .with_context(|| format!("cannot restrict {} to mode 0700", paths.dir.display()))
}

/// Print the startup lines, then open the first URL unless `--no-open`, SSH or no display.
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
        read_only: server.ctx.read_only().problems(),
    });
    for line in lines {
        println!("{line}");
    }
    if let (OpenDecision::Open, Some(url)) = (browser::decide(&env, no_open), bind.urls.first()) {
        browser::open(url);
    }
}

/// Block until the first stop signal's drains finish (then stop and return Ok), or until the
/// accept loop ends by itself (then return its result). A second signal exits at once.
fn serve_until_stopped(server: RunningServer) -> Result<()> {
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
