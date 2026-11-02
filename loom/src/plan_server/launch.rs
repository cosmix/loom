//! Starting a server: bind, start accepting and hand the rest to the background load.

use std::net::{IpAddr, TcpListener};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use anyhow::{Context, Result};

use super::context::{BindInfo, PlanContext};
use super::gate;
use super::loading::{self, CheckReadOnly, Install, Load};
use super::lock::Ownership;
use super::router::PlanRouter;
use super::routes::RouteSpec;
use super::worker_owner::{Registration, Workers};
use super::{watch, ws, RunningServer, DEFAULT_PORT};
use crate::planning::watch::{OwnWrites, Watcher};
use crate::web_server::access::AccessPolicy;
use crate::web_server::auth::Auth;
use crate::web_server::limits::Limits;
use crate::web_server::listener::bind_listener;
use crate::web_server::{self, token, ServeConfig};

/// What `start_owned` starts: [`ServerOptions`] with the steps the load runs in the background.
pub(super) struct Launch {
    pub(super) root: PathBuf,
    pub(super) host: IpAddr,
    pub(super) port: Option<u16>,
    pub(super) read_only: CheckReadOnly,
    pub(super) install: Install,
    pub(super) routes: Vec<RouteSpec>,
    pub(super) ownership: Arc<Ownership>,
}

/// Everything that must happen before the URL can be printed, and nothing else: bind, mint the
/// token, start accepting and start the background load. Reads nothing under `doc/`.
pub(super) fn start_owned(launch: Launch) -> Result<RunningServer> {
    let Launch {
        root,
        host,
        port,
        read_only,
        install,
        routes,
        ownership,
    } = launch;
    let listener = bind_listener(host, port, DEFAULT_PORT)?;
    let local = listener.local_addr()?;
    let token = token::mint().context("failed to mint the loom plan access token")?;
    let bind = BindInfo::new(local, &token);
    let ctx = Arc::new(PlanContext::unloaded(root, bind, OwnWrites::default())?);
    let running = Arc::new(AtomicBool::new(true));
    let stop = Arc::new(AtomicBool::new(false));
    let workers = Workers::launch(
        listener,
        Arc::clone(&ownership),
        Arc::clone(&ctx),
        &running,
        &stop,
        |listener, registration| {
            let serve = spawn_accept_loop(listener, &ctx, routes, &token, &running, &ownership)?;
            registration.accept(serve);
            let load = Load {
                read_only,
                install,
                running: Arc::clone(&running),
                stop: Arc::clone(&stop),
            };
            loading::spawn(&ctx, load, registration)
        },
    )?;
    Ok(RunningServer {
        local,
        token,
        ctx,
        running,
        stop,
        workers,
        ownership,
    })
}

pub(super) fn spawn_workers(
    ctx: &Arc<PlanContext>,
    watcher: Watcher,
    stop: &Arc<AtomicBool>,
    registration: &Registration,
) -> Result<()> {
    let (watcher, errors) = watch::spawn_watcher(Arc::clone(ctx), watcher, Arc::clone(stop))?;
    registration.background(watcher);
    let tick = watch::spawn_tick(Arc::clone(ctx), errors, Arc::clone(stop));
    let tick = tick.inspect_err(|_| stop.store(true, Ordering::SeqCst))?;
    registration.background(tick);
    Ok(())
}

fn spawn_accept_loop(
    listener: TcpListener,
    ctx: &Arc<PlanContext>,
    routes: Vec<RouteSpec>,
    token: &str,
    running: &Arc<AtomicBool>,
    ownership: &Arc<Ownership>,
) -> Result<JoinHandle<Result<()>>> {
    let local = listener.local_addr()?;
    let auth = Auth::new(token.to_owned(), gate::access_cookie_name(local.port()));
    let config = ServeConfig {
        policy: AccessPolicy::new(local, Some(auth)),
        limits: Limits::new(ws::MAX_INBOUND_BYTES),
        thread_name: "loom-plan-conn",
    };
    let router =
        Arc::new(PlanRouter::new(Arc::clone(ctx), routes).with_ownership(Arc::clone(ownership)));
    let running = Arc::clone(running);
    thread::Builder::new()
        .name("loom-plan-accept".into())
        .spawn(move || web_server::serve(listener, router, running, config))
        .context("cannot start the loom plan accept loop")
}
