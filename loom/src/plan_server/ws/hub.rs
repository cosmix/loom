//! Routing of planning messages to open WebSocket connections.

use std::collections::HashMap;
use std::sync::{mpsc, Arc, Mutex, MutexGuard, PoisonError};

use crate::plan_server::ids::{BrowserId, ConnCtx, ConnId};
use crate::plan_server::wire::message::PlanMessage;

/// Frames a connection may fall behind before the hub drops it.
const QUEUE_DEPTH: usize = 64;

/// A boxed callback registered with [`PlanHub::on_open`].
pub type OpenHookFn = Box<dyn Fn(&ConnCtx) -> Vec<PlanMessage> + Send + Sync>;

type OpenHook = Arc<dyn Fn(&ConnCtx) -> Vec<PlanMessage> + Send + Sync>;
type CloseHook = Arc<dyn Fn(&ConnCtx) + Send + Sync>;

struct Entry {
    browser: BrowserId,
    tx: mpsc::SyncSender<Arc<String>>,
}

/// Every open planning connection, keyed by its [`ConnId`], plus the hooks
/// that run when one opens or closes.
///
/// Delivery never blocks: each message is serialized once and `try_send`
/// to every target. A connection whose queue is full (or whose loop has
/// gone) is unregistered on the spot; dropping its sender ends its loop once
/// the queued frames are written, as the dashboard's `Hub::publish` drops
/// slow subscribers. Hooks run on a snapshot taken under the hook lock and
/// never under the connection lock, so a hook may call back into the hub.
pub struct PlanHub {
    conns: Mutex<HashMap<ConnId, Entry>>,
    open_hooks: Mutex<Vec<OpenHook>>,
    close_hooks: Mutex<Vec<CloseHook>>,
}

impl Default for PlanHub {
    fn default() -> Self {
        Self::new()
    }
}

impl PlanHub {
    pub fn new() -> Self {
        Self {
            conns: Mutex::new(HashMap::new()),
            open_hooks: Mutex::new(Vec::new()),
            close_hooks: Mutex::new(Vec::new()),
        }
    }

    /// Start routing to `conn`; the receiver yields its outbound frames.
    pub fn register(&self, conn: &ConnCtx) -> mpsc::Receiver<Arc<String>> {
        let (tx, rx) = mpsc::sync_channel(QUEUE_DEPTH);
        let entry = Entry {
            browser: conn.browser.clone(),
            tx,
        };
        lock(&self.conns).insert(conn.id, entry);
        rx
    }

    pub fn unregister(&self, id: ConnId) {
        lock(&self.conns).remove(&id);
    }

    pub fn broadcast(&self, msg: &PlanMessage) {
        self.deliver(msg, |_, _| true);
    }

    pub fn send_to_conn(&self, id: ConnId, msg: &PlanMessage) {
        self.deliver(msg, |conn, _| *conn == id);
    }

    /// Every open tab of `browser`; dropped when it has none.
    pub fn send_to_browser(&self, browser: &BrowserId, msg: &PlanMessage) {
        self.deliver(msg, |_, entry| entry.browser == *browser);
    }

    /// Every open tab of every browser but `browser`.
    pub fn send_to_others(&self, browser: &BrowserId, msg: &PlanMessage) {
        self.deliver(msg, |_, entry| entry.browser != *browser);
    }

    /// Every open tab of the authoring holder; dropped when there is no
    /// holder or the holder has no tab open. A returning tab gets current
    /// state from the `on_open` hooks.
    pub fn send_to_holder(&self, holder: Option<&BrowserId>, msg: &PlanMessage) {
        if let Some(browser) = holder {
            self.send_to_browser(browser, msg);
        }
    }

    /// Add a hook whose messages open every new connection, after the
    /// messages of hooks added before it.
    pub fn on_open(&self, f: OpenHookFn) {
        lock(&self.open_hooks).push(Arc::from(f));
    }

    pub fn on_close(&self, f: Box<dyn Fn(&ConnCtx) + Send + Sync>) {
        lock(&self.close_hooks).push(Arc::from(f));
    }

    /// Run the `on_open` hooks in order and serialize what they return.
    pub fn open_frames(&self, conn: &ConnCtx) -> Vec<Arc<String>> {
        let hooks = lock(&self.open_hooks).clone();
        hooks
            .iter()
            .flat_map(|hook| hook(conn))
            .map(|msg| Arc::new(msg.to_frame()))
            .collect()
    }

    /// Run the `on_close` hooks in order.
    pub fn closed(&self, conn: &ConnCtx) {
        let hooks = lock(&self.close_hooks).clone();
        for hook in &hooks {
            hook(conn);
        }
    }

    pub fn connections_of(&self, browser: &BrowserId) -> usize {
        lock(&self.conns)
            .values()
            .filter(|entry| entry.browser == *browser)
            .count()
    }

    /// Serialize `msg` once, queue it for every connection `target` selects,
    /// and unregister each selected connection that cannot take it.
    fn deliver(&self, msg: &PlanMessage, target: impl Fn(&ConnId, &Entry) -> bool) {
        let frame = Arc::new(msg.to_frame());
        lock(&self.conns).retain(|id, entry| {
            !target(id, entry) || entry.tx.try_send(Arc::clone(&frame)).is_ok()
        });
    }
}

/// The hub's maps stay consistent across a panic (every mutation is a
/// single insert, remove or push), so a poisoned lock is still usable.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
