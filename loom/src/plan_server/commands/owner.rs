//! The catalogue owner: one side process per chat session, started once when the session is
//! live, whose tables go to the session's command cell and to every tab, and which ends with
//! the session. A bridge listener only queues requests; this thread spawns and waits.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::Value;

use super::frame;
use crate::fs::safe_read::read_to_string_bounded;
use crate::plan_server::context::PlanContext;
use crate::plan_session::bridge::host::LaunchSpec;
use crate::plan_session::bridge::{Bridge, BridgeEvent};
use crate::plan_session::commands::{
    side_argv, side_settings, CommandTable, OnTable, SideProcess, SideSpec, STARTUP_DEADLINE,
};
use crate::plan_session::launch::script::{claude_argv, claude_env, inherited_env};

/// The most of the session's settings file the side process's copy is made from.
const SETTINGS_LIMIT: usize = 1024 * 1024;
const SETTINGS_FILE: &str = "settings.json";
const SIDE_SETTINGS_FILE: &str = "side-settings.json";
/// How often the thread looks at its stop flag while no request arrives.
const IDLE: Duration = Duration::from_millis(200);
/// What the chat shows when the side process cannot be built or started; the log has the cause.
const NOT_STARTED: &str = "The command list could not be started";

/// Builds the side process's spec from the session's launch spec.
pub(crate) type Builder = Arc<dyn Fn(&LaunchSpec) -> Result<SideSpec> + Send + Sync>;

/// What the owner thread is asked to do, queued by the bridge listener.
pub(super) enum Request {
    /// The session started: begin its catalogue if it is the live chat session.
    Begin(String),
    /// The session ended.
    End(String),
}

/// What a start uses; tests lower the deadline and swap the builder for a fake `claude`.
struct Tuning {
    deadline: Duration,
    build: Builder,
}

/// A chat session's catalogue: its side process and the latest table it reported.
#[derive(Default)]
struct Slot {
    process: Option<SideProcess>,
    table: Option<Arc<CommandTable>>,
}

/// Every chat session's catalogue, shared by the owner thread, the side processes' callbacks,
/// the route and the new-tab hook. Stopping a `SideProcess` joins its reader thread, whose
/// callback takes this lock: a slot is always taken out of the map before its process stops.
pub(crate) struct Catalogue {
    sessions: Mutex<HashMap<String, Slot>>,
    tuning: Mutex<Tuning>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Catalogue {
    pub(super) fn new() -> Self {
        let tuning = Tuning {
            deadline: STARTUP_DEADLINE,
            build: Arc::new(|spec: &LaunchSpec| side_spec(spec)),
        };
        Self {
            sessions: Mutex::default(),
            tuning: Mutex::new(tuning),
        }
    }

    /// The latest table the session's side process reported, `None` before the first.
    pub(super) fn table(&self, session: &str) -> Option<Arc<CommandTable>> {
        lock(&self.sessions).get(session)?.table.clone()
    }

    /// Lower the answer deadline of the next starts.
    #[cfg(test)]
    pub(crate) fn set_deadline(&self, deadline: Duration) {
        lock(&self.tuning).deadline = deadline;
    }

    /// Replace how the next starts build their side process.
    #[cfg(test)]
    pub(crate) fn set_builder(&self, build: Builder) {
        lock(&self.tuning).build = build;
    }

    /// The pids of the side processes alive now.
    #[cfg(test)]
    pub(crate) fn pids(&self) -> Vec<u32> {
        let sessions = lock(&self.sessions);
        let processes = sessions.values().filter_map(|slot| slot.process.as_ref());
        processes.map(SideProcess::pid).collect()
    }

    /// Make a slot for `session`; false when it has one (one start per session).
    fn claim(&self, session: &str) -> bool {
        let mut sessions = lock(&self.sessions);
        let free = !sessions.contains_key(session);
        if free {
            sessions.insert(session.to_owned(), Slot::default());
        }
        free
    }

    /// Keep the table of a session that still has a slot; false when it ended meanwhile.
    fn store(&self, session: &str, table: &Arc<CommandTable>) -> bool {
        let mut sessions = lock(&self.sessions);
        let slot = sessions.get_mut(session);
        slot.map(|slot| slot.table = Some(Arc::clone(table)))
            .is_some()
    }

    /// Give the started process to its slot; back to the caller when the session ended meanwhile.
    fn attach(&self, session: &str, process: SideProcess) -> Result<(), SideProcess> {
        let mut sessions = lock(&self.sessions);
        match sessions.get_mut(session) {
            Some(slot) => {
                slot.process = Some(process);
                Ok(())
            }
            None => Err(process),
        }
    }

    fn tuning(&self) -> (Duration, Builder) {
        let tuning = lock(&self.tuning);
        (tuning.deadline, Arc::clone(&tuning.build))
    }

    /// Stop `session`'s process and forget it.
    fn end(&self, session: &str) {
        let slot = lock(&self.sessions).remove(session);
        if let Some(process) = slot.and_then(|slot| slot.process) {
            process.stop();
        }
    }

    /// Stop every process.
    fn end_all(&self) {
        let all = std::mem::take(&mut *lock(&self.sessions));
        for process in all.into_values().filter_map(|slot| slot.process) {
            process.stop();
        }
    }
}

/// The owner thread's references; all weak, so it never keeps the context or the bridge alive
/// (a side process's callback holds a clone).
#[derive(Clone)]
struct Owner {
    ctx: Weak<PlanContext>,
    bridge: Weak<Bridge>,
    catalogue: Weak<Catalogue>,
}

/// The owner's references while a call uses them.
struct Live {
    ctx: Arc<PlanContext>,
    bridge: Arc<Bridge>,
    catalogue: Arc<Catalogue>,
}

/// Queue `Begin` and `End` requests from `bridge`'s events; returns the receiving end.
pub(super) fn listen(bridge: &Bridge) -> Receiver<Request> {
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    bridge.subscribe(Box::new(move |event| {
        let request = match event {
            BridgeEvent::Started { id, .. } => Request::Begin(id.clone()),
            BridgeEvent::Ended { id, .. } => Request::End(id.clone()),
            _ => return,
        };
        // A closed channel means the owner thread has ended; nothing is left to ask.
        let _ = lock(&tx).send(request);
    }));
    rx
}

/// Start the `command catalogue` thread over `requests`; the context stops and joins it.
pub(super) fn spawn(
    ctx: &Arc<PlanContext>,
    bridge: &Arc<Bridge>,
    catalogue: &Arc<Catalogue>,
    requests: Receiver<Request>,
) {
    let owner = Owner {
        ctx: Arc::downgrade(ctx),
        bridge: Arc::downgrade(bridge),
        catalogue: Arc::downgrade(catalogue),
    };
    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    let thread = thread::Builder::new().name("command catalogue".into());
    match thread.spawn(move || owner.run(&requests, &flag)) {
        Ok(handle) => ctx.own_worker("command catalogue", handle, Some(stop)),
        Err(error) => tracing::warn!("chat commands show no session list: {error}"),
    }
}

impl Owner {
    fn live(&self) -> Option<Live> {
        Some(Live {
            ctx: self.ctx.upgrade()?,
            bridge: self.bridge.upgrade()?,
            catalogue: self.catalogue.upgrade()?,
        })
    }

    fn run(&self, requests: &Receiver<Request>, stop: &AtomicBool) {
        while !stop.load(Ordering::SeqCst) {
            match requests.recv_timeout(IDLE) {
                Ok(Request::Begin(id)) => self.begin(&id),
                Ok(Request::End(id)) => {
                    let live = self.live();
                    if let Some(live) = live {
                        live.catalogue.end(&id);
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        // A catalogue already dropped took its processes down with it.
        let catalogue = self.catalogue.upgrade();
        if let Some(catalogue) = catalogue {
            catalogue.end_all();
        }
    }

    /// Start the chat session's side process, once, when its launch spec exists; any other
    /// session (a summary, a pressure run) starts nothing. No retry: a failure is `Unavailable`
    /// until the session ends.
    fn begin(&self, id: &str) {
        let Some(live) = self.live() else {
            return;
        };
        let is_chat = live.bridge.chat_session_id().as_deref() == Some(id);
        let Some(spec) = live.bridge.launch_spec(id).filter(|_| is_chat) else {
            return;
        };
        if !live.catalogue.claim(id) {
            return;
        }
        let (deadline, build) = live.catalogue.tuning();
        let started = build(&spec)
            .and_then(|side| SideProcess::start(side, deadline, self.on_table(id)))
            .context(NOT_STARTED);
        match started {
            Ok(process) => {
                if let Err(process) = live.catalogue.attach(id, process) {
                    process.stop();
                }
            }
            Err(error) => {
                tracing::warn!(session = %id, "no command list: {error:#}");
                self.publish(id, Arc::new(CommandTable::unavailable(NOT_STARTED)));
            }
        }
    }

    /// The callback of `id`'s side process, run on its `side-commands` thread.
    fn on_table(&self, id: &str) -> OnTable {
        let (owner, id) = (self.clone(), id.to_owned());
        Box::new(move |table| owner.publish(&id, table))
    }

    /// Keep `table`, give it to the session's gate and tell every tab.
    fn publish(&self, id: &str, table: Arc<CommandTable>) {
        let Some(live) = self.live() else {
            return;
        };
        if !live.catalogue.store(id, &table) {
            return;
        }
        if let Some(cell) = live.bridge.command_cell(id) {
            *cell.write().unwrap_or_else(PoisonError::into_inner) = Arc::clone(&table);
        }
        live.ctx.hub().broadcast(&frame(id, &table));
    }
}

/// The side process's spec for `spec`'s session: its own command line without the identity,
/// name and brief, a sibling settings file in the session directory without hooks, and the
/// session's environment without its planning identity.
fn side_spec(spec: &LaunchSpec) -> Result<SideSpec> {
    let file = Path::new(SETTINGS_FILE);
    let text = read_to_string_bounded(spec.dir.path(), file, SETTINGS_LIMIT)?;
    let settings: Value = serde_json::from_str(&text).context("the session's settings")?;
    let settings_path = spec.dir.path().join(SIDE_SETTINGS_FILE);
    let session = claude_argv(spec, &spec.dir.settings());
    let args = side_argv(&session, &settings_path).into_iter().skip(1);
    let identity = ["LOOM_PLAN_SESSION", "LOOM_PLAN_STATE"];
    let env: Vec<(OsString, OsString)> = claude_env(spec, &inherited_env(spec))
        .into_iter()
        .filter(|(name, _)| !identity.iter().any(|key| name == key))
        .collect();
    Ok(SideSpec {
        program: spec.claude_bin.clone(),
        args: args.collect(),
        env,
        cwd: spec.project_root.clone(),
        settings_path,
        settings: side_settings(&settings),
    })
}

#[cfg(test)]
#[path = "owner_tests.rs"]
mod tests;
