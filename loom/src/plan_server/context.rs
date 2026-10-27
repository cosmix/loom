//! The planning server's shared state, handed to every route, hook and connection.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::SystemTime;

use anyhow::Result;

use super::authoring::{Authoring, Refusal, Take};
use super::extensions::Extensions;
use super::ids::{BrowserId, ConnCtx, ConnId, PublicId};
use super::prereq::ReadOnly;
use super::presence::Presence;
use super::shutdown::Shutdown;
use super::wire::checklist::ChecklistView;
use super::wire::message::{Hello, PlanMessage};
use super::wire::server::{ServerInfo, You};
use super::ws::PlanHub;
use crate::planning::project::Project;
use crate::planning::runtime::{project_key, server_runtime_paths, RuntimePaths};
use crate::planning::watch::{Batch, OwnWrites};
use crate::web_server::interfaces::matching_addresses;

/// Where the server listens and the tokenized URLs that reach it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindInfo {
    /// The bound listener address (a wildcard address for a wildcard bind).
    pub local: SocketAddr,
    /// True for every non-loopback bind, wildcard included.
    pub remote: bool,
    /// `http://<addr>/?token=<token>`, one per address a browser can use.
    pub urls: Vec<String>,
}

impl BindInfo {
    pub(crate) fn new(local: SocketAddr, token: &str) -> Self {
        let urls = display_addrs(local)
            .iter()
            .map(|addr| format!("http://{addr}/?token={token}"))
            .collect();
        BindInfo {
            local,
            remote: !local.ip().is_loopback(),
            urls,
        }
    }
}

/// The addresses a browser reaches `local` at: `local` itself, or every interface address of
/// a wildcard bind.
pub(crate) fn display_addrs(local: SocketAddr) -> Vec<SocketAddr> {
    if !local.ip().is_unspecified() {
        return vec![local];
    }
    matching_addresses(local.ip())
        .into_iter()
        .map(|ip| SocketAddr::new(ip, local.port()))
        .collect()
}

/// A boxed callback registered with [`PlanContext::on_batch`].
pub type BatchHookFn = Box<dyn Fn(&PlanContext, &Batch) + Send + Sync>;

type BatchHook = Arc<dyn Fn(&PlanContext, &Batch) + Send + Sync>;

/// Everything one planning server shares. Built once by `start`; `Send + Sync`.
pub struct PlanContext {
    root: PathBuf,
    project_key: String,
    runtime: RuntimePaths,
    project: RwLock<Project>,
    authoring: Authoring,
    presence: Presence,
    hub: PlanHub,
    shutdown: Arc<Shutdown>,
    read_only: ReadOnly,
    bind: BindInfo,
    own_writes: OwnWrites,
    batch_hooks: RwLock<Vec<BatchHook>>,
    extensions: Extensions,
    next_conn: AtomicU64,
}

impl PlanContext {
    /// Load the project under `root` and compute its runtime paths (nothing is created).
    pub(crate) fn new(root: PathBuf, read_only: ReadOnly, bind: BindInfo) -> Result<Self> {
        let project = Project::load(&root)?;
        let (runtime, _) = server_runtime_paths(&root)?;
        Ok(PlanContext {
            project_key: project_key(&root)?,
            root,
            runtime,
            project: RwLock::new(project),
            authoring: Authoring::new(read_only.is_read_only()),
            presence: Presence::new(),
            hub: PlanHub::new(),
            shutdown: Arc::new(Shutdown::new()),
            read_only,
            bind,
            own_writes: OwnWrites::default(),
            batch_hooks: RwLock::new(Vec::new()),
            extensions: Extensions::default(),
            next_conn: AtomicU64::new(1),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn runtime(&self) -> &RuntimePaths {
        &self.runtime
    }

    /// `planning::runtime::project_key(root)`.
    pub fn project_key(&self) -> &str {
        &self.project_key
    }

    /// The loaded project. Hold the guard briefly: the watcher waits on it to refresh.
    pub fn project(&self) -> RwLockReadGuard<'_, Project> {
        self.project.read().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn project_mut(&self) -> RwLockWriteGuard<'_, Project> {
        self.project.write().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn authoring(&self) -> &Authoring {
        &self.authoring
    }

    pub fn presence(&self) -> &Presence {
        &self.presence
    }

    pub fn hub(&self) -> &PlanHub {
        &self.hub
    }

    pub fn shutdown(&self) -> &Shutdown {
        &self.shutdown
    }

    /// The shared handle the signal handler aborts through.
    pub(crate) fn shutdown_handle(&self) -> Arc<Shutdown> {
        Arc::clone(&self.shutdown)
    }

    pub fn read_only(&self) -> &ReadOnly {
        &self.read_only
    }

    pub fn bind(&self) -> &BindInfo {
        &self.bind
    }

    /// True from the first stop signal on; routes and commands refuse new work themselves.
    pub fn is_stopping(&self) -> bool {
        self.shutdown.is_stopping()
    }

    /// [`Authoring::require`] for `browser`, with its public id and the stopping flag.
    pub fn require_authoring(&self, browser: &BrowserId, take: Take) -> Result<(), Refusal> {
        let public_id = self.presence.public_id(browser);
        self.authoring
            .require(browser, &public_id, take, self.is_stopping())
    }

    /// The text a refused browser is shown.
    pub fn refusal_message(&self, refusal: &Refusal) -> String {
        match refusal {
            Refusal::NotHolder { .. } => {
                let holder = self.authoring.view(&self.presence).holder;
                match holder {
                    Some(holder) => format!("Another browser holds authoring ({})", holder.name),
                    None => "Another browser holds authoring".to_owned(),
                }
            }
            Refusal::NoHolder => "Nobody holds authoring".to_owned(),
            Refusal::ReadOnlyMode => "loom plan is in read-only mode".to_owned(),
            Refusal::Stopping => "loom plan is stopping".to_owned(),
        }
    }

    /// Run `f` after every watcher batch has refreshed the project, in registration order.
    pub fn on_batch(&self, f: BatchHookFn) {
        self.batch_hooks
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Arc::from(f));
    }

    /// A snapshot of the batch hooks, so a hook may register another without deadlocking.
    pub(crate) fn batch_hooks(&self) -> Vec<BatchHook> {
        self.batch_hooks
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The watcher's own-write registry, shared with every engine that writes under `doc/`.
    pub fn own_writes(&self) -> &OwnWrites {
        &self.own_writes
    }

    pub fn extensions(&self) -> &Extensions {
        &self.extensions
    }

    /// A new connection's context for `browser`.
    pub(crate) fn conn_ctx(&self, browser: BrowserId) -> ConnCtx {
        ConnCtx {
            id: ConnId(self.next_conn.fetch_add(1, Ordering::Relaxed)),
            public_id: self.presence.public_id(&browser),
            browser,
        }
    }

    /// What `GET /api/server` and `hello` tell `browser` (whose public id this request or
    /// connection uses) about the server. Records nothing: a browser not yet recorded shows an
    /// empty name.
    pub fn server_info(&self, browser: &BrowserId, public_id: &PublicId) -> ServerInfo {
        let project = self
            .root
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.root.display().to_string());
        ServerInfo {
            project,
            project_key: self.project_key.clone(),
            port: self.bind.local.port(),
            remote: self.bind.remote,
            read_only: self.read_only.problems().to_vec(),
            stopping: self.is_stopping(),
            you: You {
                public_id: public_id.clone(),
                name: self.presence.name_of(browser),
            },
        }
    }

    /// Send the presence list to every connection.
    pub fn broadcast_presence(&self) {
        let list = self.presence.list(&self.authoring);
        self.hub.broadcast(&PlanMessage::Presence(list));
    }

    /// Send the authoring view and the presence list (which marks the holder) to everyone.
    pub fn broadcast_people(&self) {
        let view = self.authoring.view(&self.presence);
        self.hub.broadcast(&PlanMessage::Authoring(view));
        self.broadcast_presence();
    }

    /// Wire presence and authoring to the hub: an opening connection is counted, then gets
    /// `hello`; a closing one is uncounted; every change reaches every connection. The hooks
    /// hold a weak reference, so the context is not kept alive by its own hub.
    pub(crate) fn install_hub_hooks(ctx: &Arc<PlanContext>) {
        let weak = Arc::downgrade(ctx);
        ctx.hub.on_open(Box::new(move |conn| {
            weak.upgrade()
                .map(|ctx| ctx.connection_opened(conn))
                .unwrap_or_default()
        }));
        let weak = Arc::downgrade(ctx);
        ctx.hub.on_close(Box::new(move |conn| {
            if let Some(ctx) = weak.upgrade() {
                ctx.connection_closed(conn);
            }
        }));
        let weak = Arc::downgrade(ctx);
        ctx.authoring.subscribe(Box::new(move |_| {
            if let Some(ctx) = weak.upgrade() {
                ctx.broadcast_people();
            }
        }));
    }

    fn connection_opened(&self, conn: &ConnCtx) -> Vec<PlanMessage> {
        self.presence.opened(conn);
        self.authoring.connection_opened(&conn.browser);
        let hello = self.hello(conn);
        self.broadcast_presence();
        vec![PlanMessage::Hello(Box::new(hello))]
    }

    fn connection_closed(&self, conn: &ConnCtx) {
        self.presence.closed(conn.id);
        self.authoring
            .connection_closed(&conn.browser, SystemTime::now());
        self.broadcast_presence();
    }

    fn hello(&self, conn: &ConnCtx) -> Hello {
        let checklist = ChecklistView::from(&self.project().checklist);
        let server = self.server_info(&conn.browser, &conn.public_id);
        Hello {
            you: server.you.clone(),
            server,
            checklist,
            presence: self.presence.list(&self.authoring),
            authoring: self.authoring.view(&self.presence),
        }
    }
}
