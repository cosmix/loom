//! Chat wiring of the planning server: installs the bridge, dispatches browser chat commands and publishes chat frames.

mod dispatch;
pub mod guidance;
mod kickoff;
mod kickoff_cmd;
pub mod lease;
pub mod pins;
mod publish;
mod team;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_dispatch;

pub use dispatch::dispatch;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, TryLockError};
use std::thread;
use std::time::Duration;

use self::guidance::{ChecklistGuidance, GuidanceSlot};
use self::lease::AuthoringLeases;
use self::pins::TextPins;
use super::authoring::{AuthoringEvent, FreeReason};
use super::context::PlanContext;
use super::files::texts::TextStore;
use super::ids::{BrowserId, PublicId};
use super::lifecycle;
use super::shutdown::ShutdownHook;
use super::wire::{chat::ChatFrame, message::PlanMessage, team::DraftTeamView};
use crate::plan_session::bridge::host::SessionHost;
use crate::plan_session::bridge::registry::read_record;
use crate::plan_session::bridge::{
    Bridge, BridgeConfig, BridgeEvent, EndCause, EndMode, EndOutcome, TurnPins, TEARDOWN_FAILED,
};
use crate::plan_session::kind::{SessionDir, SessionKind};
use crate::plan_session::model::{ChatDelta, SessionView};
use crate::plan_session::tmux_host::TmuxHost;
use crate::planning::runtime::RuntimePaths;

/// What the chat wiring remembers between commands and events.
#[derive(Default)]
struct ChatState {
    /// The browser that started each authoring chat, by session id (px-state-R02).
    starters: Mutex<HashMap<String, PublicId>>,
    /// The last browser seen holding authoring: the hub addresses the holder by browser.
    holder: Mutex<Option<BrowserId>>,
    /// Held by a start from its take of authoring to its give-back, so another start's take,
    /// reserve and listing check cannot fall in between (dispatch's `give_back`).
    starting: Mutex<()>,
    kickoff: Mutex<kickoff::KickoffTracker>,
    kickoff_scopes: Mutex<HashMap<String, bool>>,
    team: Mutex<Option<DraftTeamView>>,
}

impl ChatState {
    /// Never taken by the guidance or a bridge listener, which run inside a start.
    fn starting(&self) -> MutexGuard<'_, ()> {
        lock(&self.starting)
    }

    /// What a bridge listener takes instead: `None` while a start holds it.
    fn try_starting(&self) -> Option<MutexGuard<'_, ()>> {
        match self.starting.try_lock() {
            Ok(guard) => Some(guard),
            Err(TryLockError::Poisoned(poisoned)) => Some(poisoned.into_inner()),
            Err(TryLockError::WouldBlock) => None,
        }
    }

    fn started(&self, id: String, by: PublicId) {
        lock(&self.starters).insert(id, by);
    }

    fn take_starter(&self, id: &str) -> Option<PublicId> {
        lock(&self.starters).remove(id)
    }

    fn remember_holder(&self, browser: &BrowserId) {
        *lock(&self.holder) = Some(browser.clone());
    }

    /// The remembered browser while it still holds authoring.
    fn holder(&self, ctx: &PlanContext) -> Option<BrowserId> {
        let remembered = lock(&self.holder).clone();
        remembered.filter(|browser| ctx.authoring().is_holder(browser))
    }
}

/// Stops the bridge's driver when the context, which owns this through its extensions, drops.
struct DriverStop(Arc<AtomicBool>);

impl Drop for DriverStop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

/// Build the bridge over tmux and wire it to the hub, authoring and shutdown.
pub fn install(ctx: &Arc<PlanContext>) {
    install_with(ctx, Arc::new(TmuxHost), |_config| {
        #[cfg(not(test))]
        {
            _config.claude_bin = crate::claude::find_claude_path().unwrap_or_default();
        }
    });
}

/// [`install`] over `host`, with `config_overrides` applied to the bridge's configuration.
pub(crate) fn install_with(
    ctx: &Arc<PlanContext>,
    host: Arc<dyn SessionHost>,
    config_overrides: impl FnOnce(&mut BridgeConfig),
) {
    let slot = Arc::new(GuidanceSlot::new(Arc::new(ChecklistGuidance)));
    let leases = Arc::new(AuthoringLeases::default());
    let state = Arc::new(ChatState::default());
    let mut config = bridge_config(ctx, &slot);
    config_overrides(&mut config);
    let runtime = config.runtime.clone();
    let bridge = Bridge::new(config, host, turn_pins(ctx));
    ctx.extensions().insert(slot);
    ctx.extensions().insert(Arc::clone(&leases));
    ctx.extensions().insert(Arc::clone(&state));
    ctx.extensions().insert(Arc::clone(&bridge));
    publish::subscribe(ctx, &bridge, &state);
    open_snapshots(ctx, &bridge, &state);
    follow_authoring(ctx, &bridge);
    follow_sessions(ctx, &bridge, state, leases, runtime);
    let (drain, abort) = (Arc::clone(&bridge), Arc::clone(&bridge));
    ctx.shutdown().register(ShutdownHook {
        name: "chat sessions",
        drain: Box::new(move || drain.drain()),
        abort: Box::new(move || abort.abort()),
    });
    let stop = Arc::new(AtomicBool::new(false));
    // Detached: `DriverStop` ends the driver with the context.
    drop(bridge.run(Arc::clone(&stop)));
    ctx.extensions().insert(Arc::new(DriverStop(stop)));
    spawn_recovery(&bridge);
}

fn bridge_config(ctx: &PlanContext, slot: &Arc<GuidanceSlot>) -> BridgeConfig {
    let loom_bin = std::env::current_exe().and_then(|exe| exe.canonicalize());
    let loom_bin = loom_bin.unwrap_or_else(|error| {
        tracing::warn!("chat sessions cannot run their hooks: no loom binary path: {error}");
        PathBuf::new()
    });
    let slot = Arc::clone(slot);
    BridgeConfig {
        project_root: ctx.root().to_path_buf(),
        runtime: ctx.runtime().clone(),
        loom_bin,
        claude_bin: PathBuf::new(),
        // Unset or empty: the bridge refuses every start with "HOME is not set".
        home: std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default(),
        // Read when delivered, so a provider set later (plan 8) supplies the text.
        closing_message: Arc::new(move || slot.get().closing_message()),
        delivery_timeout: Duration::from_secs(10),
        final_write_bound: Duration::from_secs(600),
        permission_deny_after: Duration::from_secs(3590),
        identity_interval: Duration::from_secs(2),
        tick: Duration::from_millis(200),
    }
}

/// Turn diffs need plan 3's text store; without it turns carry no changed files.
fn turn_pins(ctx: &Arc<PlanContext>) -> Option<Arc<dyn TurnPins>> {
    let store = ctx.extensions().get::<TextStore>()?;
    Some(Arc::new(TextPins::new(store, Arc::downgrade(ctx))))
}

/// The bridge leaves the holder out of the view; it comes from authoring and presence.
fn with_holder(ctx: &PlanContext, mut session: SessionView) -> SessionView {
    let holder = ctx.authoring().view(ctx.presence()).holder;
    session.holder = holder.as_ref().map(|holder| holder.name.clone());
    session.away_until = holder.and_then(|holder| holder.frees_at);
    session
}

/// A new tab opens with the chat snapshot; queued texts only for the holder.
fn open_snapshots(ctx: &Arc<PlanContext>, bridge: &Arc<Bridge>, state: &Arc<ChatState>) {
    let (weak, bridge, state) = (Arc::downgrade(ctx), Arc::clone(bridge), Arc::clone(state));
    ctx.hub().on_open(Box::new(move |conn| {
        let Some(ctx) = weak.upgrade() else {
            return Vec::new();
        };
        let holder = ctx.authoring().is_holder(&conn.browser);
        if holder {
            state.remember_holder(&conn.browser);
        }
        let mut snapshot = bridge.chat_snapshot(holder);
        snapshot.session = snapshot.session.map(|view| with_holder(&ctx, view));
        let mut messages = vec![PlanMessage::Chat(Box::new(ChatFrame::Snapshot(snapshot)))];
        messages.extend(team::snapshots(&state));
        messages
    }));
}

fn follow_authoring(ctx: &Arc<PlanContext>, bridge: &Arc<Bridge>) {
    let (weak, bridge) = (Arc::downgrade(ctx), Arc::clone(bridge));
    ctx.authoring().subscribe(Box::new(move |event| {
        let Some(ctx) = weak.upgrade() else {
            return;
        };
        if matches!(
            event,
            AuthoringEvent::Freed {
                reason: FreeReason::GraceExpired
            }
        ) {
            end_abandoned_chat(&bridge);
        }
        // The session view names the holder: every tab hears when that changes.
        if let Some(session) = bridge.chat_snapshot(false).session {
            let session = with_holder(&ctx, session);
            let delta = ChatDelta::Session { session };
            ctx.hub().broadcast(&publish::delta_message(delta));
        }
    }));
}

/// Grace expiry ends an authoring chat with a final write; a pressure run is plan 11's to end.
fn end_abandoned_chat(bridge: &Bridge) {
    let Some(id) = bridge.chat_session_id() else {
        return;
    };
    let mut sessions = bridge.sessions().into_iter();
    if !sessions.any(|(listed, kind, _)| listed == id && kind == SessionKind::Authoring) {
        return;
    }
    if let Err(error) = bridge.end(&id, EndMode::FinalWrite, EndCause::GraceExpired) {
        tracing::debug!(session = %id, %error, "grace expiry did not end the chat");
    }
}

fn follow_sessions(
    ctx: &Arc<PlanContext>,
    bridge: &Arc<Bridge>,
    state: Arc<ChatState>,
    leases: Arc<AuthoringLeases>,
    runtime: RuntimePaths,
) {
    // Weak: the bridge owns this listener.
    let (weak, events) = (Arc::downgrade(ctx), Arc::downgrade(bridge));
    bridge.subscribe(Box::new(move |event| {
        let (Some(ctx), Some(bridge)) = (weak.upgrade(), events.upgrade()) else {
            return;
        };
        match event {
            BridgeEvent::Started { id, kind } => {
                let (name, socket) = describe(&bridge, &runtime, id, kind);
                lifecycle::line(&started_line(kind, &name, &socket));
            }
            BridgeEvent::Team { id, teammates } => {
                team::publish_draft_team(&ctx, &state, id, teammates);
            }
            BridgeEvent::Ended { id, kind, outcome } => {
                team::clear(&state, id);
                let (name, socket) = describe(&bridge, &runtime, id, kind);
                lifecycle::line(&ended_line(id, kind, &name, &socket, outcome));
                let starter = state.take_starter(id);
                // Only the starter's hold ends with its chat, and never under a lease (px-state-R02).
                if let Some(starter) = starter.filter(|_| *kind == SessionKind::Authoring) {
                    dispatch::release_ended(&ctx, &state, &bridge, &leases, starter);
                }
            }
            _ => {}
        }
    }));
}

/// The session's display name and tmux socket: the chat's view, else its record.
fn describe(
    bridge: &Bridge,
    runtime: &RuntimePaths,
    id: &str,
    kind: &SessionKind,
) -> (String, String) {
    let snapshot = bridge.chat_snapshot(false);
    let previous = snapshot.previous.map(|ended| ended.session);
    if let Some(view) = snapshot
        .session
        .into_iter()
        .chain(previous)
        .find(|v| v.id == id)
    {
        return (view.name, view.socket);
    }
    let record = read_record(&SessionDir::new(runtime.sessions.join(id)));
    let socket = record.map(|record| record.socket).unwrap_or_default();
    (kind.label().to_owned(), socket)
}

fn started_line(kind: &SessionKind, name: &str, socket: &str) -> String {
    format!(
        "session started: {} \"{}\" (watch: tmux -L {} attach -r)",
        lifecycle::safe(kind.label()),
        lifecycle::safe(name),
        lifecycle::safe(socket)
    )
}

/// A failed teardown keeps the session and retries it, so it gets its own line.
fn ended_line(
    id: &str,
    kind: &SessionKind,
    name: &str,
    socket: &str,
    outcome: &EndOutcome,
) -> String {
    let outcome = match outcome {
        EndOutcome::Failed(why) if why.starts_with(TEARDOWN_FAILED) => {
            let (id, socket) = (lifecycle::safe(id), lifecycle::safe(socket));
            return format!("{TEARDOWN_FAILED}: session {id}, socket {socket}");
        }
        EndOutcome::Failed(why) => format!("failed: {why}"),
        EndOutcome::Closed => "closed".to_owned(),
        EndOutcome::Marker => "marker".to_owned(),
        EndOutcome::TornDown => "torn down".to_owned(),
    };
    format!(
        "session ended: {} \"{}\" ({})",
        lifecycle::safe(kind.label()),
        lifecycle::safe(name),
        lifecycle::safe(&outcome)
    )
}

/// Recovery runs before the first chat can start (the bridge refuses starts until it ends).
fn spawn_recovery(bridge: &Arc<Bridge>) {
    let bridge = Arc::clone(bridge);
    let spawned = thread::Builder::new()
        .name("loom-plan-recover".into())
        .spawn(move || {
            for (id, socket, why) in bridge.recover().left_alone {
                let (id, socket) = (lifecycle::safe(&id), lifecycle::safe(&socket));
                let why = lifecycle::safe(&why);
                lifecycle::line(&format!("left alone: session {id}, socket {socket}: {why}"));
            }
        });
    if let Err(error) = spawned {
        tracing::warn!("chats stay unavailable: session recovery could not start: {error}");
    }
}

/// Every mutation is a single insert, remove or store (`starting` guards no data), so a
/// poisoned lock is still usable.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod line_tests {
    use super::{ended_line, started_line, EndOutcome, SessionKind};

    #[test]
    fn the_terminal_lines_name_the_socket_and_neutralise_escapes() {
        let (kind, socket) = (SessionKind::Authoring, "loom-plan-3f9a");
        let started = started_line(&kind, "chat \x1b]0;owned\x07", socket);
        assert!(started.starts_with("session started: authoring \"chat "));
        assert!(started.ends_with("(watch: tmux -L loom-plan-3f9a attach -r)"));
        assert!(!started.contains(['\x1b', '\x07']), "{started:?}");
        let failed = EndOutcome::Failed("teardown failed: kill-server failed".into());
        let line = ended_line("id-1", &kind, "chat", socket, &failed);
        assert_eq!(line, "teardown failed: session id-1, socket loom-plan-3f9a");
        let closed = ended_line("id", &SessionKind::Summary, "s", "x", &EndOutcome::Closed);
        assert_eq!(closed, "session ended: summary \"s\" (closed)");
    }
}
