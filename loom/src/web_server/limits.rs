//! Admission control for connection threads, and the inbound WebSocket size
//! bound a server configures.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Connection threads allowed in flight at once.
///
/// Each one costs a 16 KiB request-head buffer plus its stack, and nothing
/// bounds how many clients a local process may open, so the accept loop turns
/// clients away past this point instead of spawning without limit.
pub(crate) const MAX_CONNECTIONS: usize = 64;

/// How many of [`MAX_CONNECTIONS`] may be WebSocket subscriptions at once.
///
/// Every HTTP response carries `Connection: close`, so those slots turn over
/// in milliseconds, but a `/ws` subscription holds its one for as long as the
/// tab stays open. Sharing a single pool would let open tabs starve the page
/// they were opened from: with the pool full, the next `GET /` is answered
/// with a 503 whose explanation only ever reaches a client that already has
/// the dashboard loaded. The difference between the two constants is the
/// reserve that non-upgrade requests always draw on.
pub(crate) const MAX_WEBSOCKETS: usize = 48;

/// How many of [`MAX_CONNECTIONS`] may be requests a router holds back for a
/// slow startup at once.
///
/// A held request keeps its connection slot for as long as the hold lasts (up
/// to a minute for the planning server), so it is not one of the slots that
/// turn over in milliseconds. Capping the hold at the same figure as
/// [`MAX_WEBSOCKETS`] leaves the same reserve for requests that need nothing
/// loaded: the page shell and its assets.
pub(crate) const MAX_HELD: usize = 48;

/// Browser terminals allowed in flight at once.
pub(crate) const MAX_TERMINALS: usize = 8;

/// The lanes a connection can occupy a slot in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lane {
    /// Held for the whole life of a connection thread, upgraded or not.
    Connection,
    /// Held additionally, for the life of the subscription, by `/ws`.
    WebSocket,
    /// Held additionally by one browser terminal.
    Terminal,
    /// Held additionally, while a router keeps the request waiting.
    Held,
}

impl Lane {
    fn capacity(self) -> usize {
        match self {
            Self::Connection => MAX_CONNECTIONS,
            Self::WebSocket => MAX_WEBSOCKETS,
            Self::Terminal => MAX_TERMINALS,
            Self::Held => MAX_HELD,
        }
    }

    fn counter(self, limits: &Limits) -> &AtomicUsize {
        match self {
            Self::Connection => &limits.connections,
            Self::WebSocket => &limits.websockets,
            Self::Terminal => &limits.terminals,
            Self::Held => &limits.held,
        }
    }
}

/// The server's live occupancy of every lane, shared by every connection, and
/// the largest inbound WebSocket message it accepts.
#[derive(Debug)]
pub struct Limits {
    connections: AtomicUsize,
    websockets: AtomicUsize,
    terminals: AtomicUsize,
    held: AtomicUsize,
    max_inbound_bytes: usize,
}

impl Limits {
    /// Empty lanes, with `max_inbound_bytes` as the largest inbound message
    /// and frame a WebSocket lane accepts from a browser. Lanes configure it
    /// as tungstenite's `max_message_size` and `max_frame_size`, so its
    /// 64 MiB / 16 MiB defaults cannot let a client make a connection thread
    /// hold that much for nothing.
    pub fn new(max_inbound_bytes: usize) -> Arc<Self> {
        Arc::new(Self {
            connections: AtomicUsize::new(0),
            websockets: AtomicUsize::new(0),
            terminals: AtomicUsize::new(0),
            held: AtomicUsize::new(0),
            max_inbound_bytes,
        })
    }

    /// The largest inbound WebSocket message and frame, in bytes.
    pub fn max_inbound_bytes(&self) -> usize {
        self.max_inbound_bytes
    }

    pub(crate) fn connection_count(&self) -> usize {
        self.connections.load(Ordering::SeqCst)
    }

    #[cfg(test)]
    pub(crate) fn terminal_count(&self) -> usize {
        self.terminals.load(Ordering::SeqCst)
    }
}

/// Reserve a terminal lane slot for a browser terminal.
pub(crate) fn acquire_terminal_slot(limits: &Arc<Limits>) -> Option<Slot> {
    Slot::acquire(limits, Lane::Terminal)
}

/// A reserved slot in one lane, released when the holder is dropped.
///
/// The release runs in `Drop`, so a panicking connection thread returns its
/// slots as well.
pub(crate) struct Slot {
    limits: Arc<Limits>,
    lane: Lane,
}

impl Slot {
    /// Reserve a slot in `lane`, or return `None` once it is full.
    ///
    /// The increment and the capacity test are one read-modify-write, so two
    /// threads racing at the boundary cannot both see room: the loser reads a
    /// count at or above the cap and rolls its own increment back.
    pub(crate) fn acquire(limits: &Arc<Limits>, lane: Lane) -> Option<Self> {
        let counter = lane.counter(limits);
        let taken = counter.fetch_add(1, Ordering::SeqCst);
        if taken >= lane.capacity() {
            counter.fetch_sub(1, Ordering::SeqCst);
            return None;
        }
        Some(Self {
            limits: limits.clone(),
            lane,
        })
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.lane
            .counter(&self.limits)
            .fetch_sub(1, Ordering::SeqCst);
    }
}
