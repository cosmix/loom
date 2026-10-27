//! Authoring holder: which browser may chat and save, and when that right frees.
//!
//! The holder is a `BrowserId` (the HttpOnly browser cookie value), so every tab of the holding
//! browser holds authoring. The holder is away once its last connection closes and frees
//! `GRACE` later unless one of its tabs reconnects. Time is always passed in so tests drive it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime};

use chrono::{DateTime, SecondsFormat, Utc};

use crate::plan_server::ids::{BrowserId, PublicId};
use crate::plan_server::presence::Presence;
use crate::plan_server::wire::presence::{AuthoringView, HolderView};

/// How long an away holder keeps authoring (section 07, "Grace period: 10 minutes").
pub const GRACE: Duration = Duration::from_secs(600);

/// What a request may do about authoring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Take {
    /// Only the current holder passes; nobody ever takes authoring this way.
    HolderOnly,
    /// The holder passes; when nobody holds, the requesting browser takes authoring.
    TakeIfFree,
}

/// Why a request that needs authoring was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Another browser holds authoring (away or not).
    NotHolder { holder: PublicId },
    /// A holder-only request while nobody holds authoring.
    NoHolder,
    /// The server runs in read-only mode, so nobody can ever take authoring.
    ReadOnlyMode,
    /// `loom plan` is stopping and takes no new authoring work.
    Stopping,
}

/// Why authoring freed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FreeReason {
    GraceExpired,
    Released(&'static str),
}

/// A change to the holder, delivered to every subscriber after the internal lock is dropped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthoringEvent {
    Taken { public_id: PublicId },
    Away { since: SystemTime },
    Back,
    Freed { reason: FreeReason },
}

type Subscriber = Arc<dyn Fn(&AuthoringEvent) + Send + Sync>;

struct Holder {
    browser: BrowserId,
    public_id: PublicId,
    away_since: Option<SystemTime>,
}

#[derive(Default)]
struct State {
    holder: Option<Holder>,
    /// Open connections per browser; a browser with none has no entry.
    open: HashMap<BrowserId, u32>,
}

/// The single authoring holder of one planning server. `Send + Sync`; share it by reference.
pub struct Authoring {
    read_only: bool,
    state: Mutex<State>,
    subscribers: Mutex<Vec<Subscriber>>,
}

impl Authoring {
    pub fn new(read_only: bool) -> Self {
        Self {
            read_only,
            state: Mutex::new(State::default()),
            subscribers: Mutex::new(Vec::new()),
        }
    }

    /// Check that `browser` may act under `policy`, taking authoring when `TakeIfFree` and nobody
    /// holds. Read-only mode and `stopping` refuse every `TakeIfFree`, the holder's included (the
    /// composer is disabled while stopping); `HolderOnly` ignores `stopping` so the holder can
    /// still answer prompts during the final write.
    ///
    /// A browser that takes authoring with no open connection is not marked away until one of
    /// its connections opens and closes, because no time is passed here. The planning app keeps a
    /// WebSocket open in every tab, so this only matters for a client that never connects one;
    /// revisit if a route that takes authoring is meant for such clients.
    pub fn require(
        &self,
        browser: &BrowserId,
        public_id: &PublicId,
        policy: Take,
        stopping: bool,
    ) -> Result<(), Refusal> {
        let mut state = self.lock_state();
        if policy == Take::TakeIfFree {
            if self.read_only {
                return Err(Refusal::ReadOnlyMode);
            }
            if stopping {
                return Err(Refusal::Stopping);
            }
        }
        match &state.holder {
            Some(holder) if &holder.browser == browser => Ok(()),
            Some(holder) => Err(Refusal::NotHolder {
                holder: holder.public_id.clone(),
            }),
            None if policy == Take::HolderOnly && self.read_only => Err(Refusal::ReadOnlyMode),
            None if policy == Take::HolderOnly => Err(Refusal::NoHolder),
            None => {
                state.holder = Some(Holder {
                    browser: browser.clone(),
                    public_id: public_id.clone(),
                    away_since: None,
                });
                drop(state);
                self.fire(&AuthoringEvent::Taken {
                    public_id: public_id.clone(),
                });
                Ok(())
            }
        }
    }

    pub fn is_holder(&self, browser: &BrowserId) -> bool {
        self.lock_state()
            .holder
            .as_ref()
            .is_some_and(|holder| &holder.browser == browser)
    }

    /// The holder's public id and, while it is away, when its last connection closed.
    pub fn holder(&self) -> Option<(PublicId, Option<SystemTime>)> {
        self.lock_state()
            .holder
            .as_ref()
            .map(|holder| (holder.public_id.clone(), holder.away_since))
    }

    /// The wire view of the holder, named through `names`.
    pub fn view(&self, names: &Presence) -> AuthoringView {
        let held = self.lock_state().holder.as_ref().map(|holder| {
            (
                holder.browser.clone(),
                holder.public_id.clone(),
                holder.away_since,
            )
        });
        // The authoring lock is dropped before `names` is locked: no lock order to keep.
        let holder = held.map(|(browser, public_id, away_since)| HolderView {
            public_id,
            name: names.name_of(&browser),
            away_since: away_since.map(wire_time),
            frees_at: away_since
                .and_then(|since| since.checked_add(GRACE))
                .map(wire_time),
        });
        AuthoringView { holder }
    }

    /// Free authoring whoever holds it; nothing happens while nobody holds.
    pub fn release(&self, reason: FreeReason) {
        let freed = self.lock_state().holder.take().is_some();
        if freed {
            self.fire(&AuthoringEvent::Freed { reason });
        }
    }

    /// Free authoring only while `holder` still holds it; returns whether it freed.
    pub fn release_if(&self, holder: &PublicId, reason: FreeReason) -> bool {
        let mut state = self.lock_state();
        let held_by_them = state
            .holder
            .as_ref()
            .is_some_and(|current| &current.public_id == holder);
        if !held_by_them {
            return false;
        }
        state.holder = None;
        drop(state);
        self.fire(&AuthoringEvent::Freed { reason });
        true
    }

    /// Register `f` for every later event. Callbacks run on the thread that caused the event,
    /// with no internal lock held, so they may call back into this `Authoring`.
    pub fn subscribe(&self, f: Box<dyn Fn(&AuthoringEvent) + Send + Sync>) {
        self.lock_subscribers().push(Arc::from(f));
    }

    /// A connection of `browser` opened; the holder coming back while away fires `Back`.
    pub fn connection_opened(&self, browser: &BrowserId) {
        let mut state = self.lock_state();
        *state.open.entry(browser.clone()).or_insert(0) += 1;
        let back = match state.holder.as_mut() {
            Some(holder) if &holder.browser == browser && holder.away_since.is_some() => {
                holder.away_since = None;
                true
            }
            _ => false,
        };
        drop(state);
        if back {
            self.fire(&AuthoringEvent::Back);
        }
    }

    /// A connection of `browser` closed at `now`; the holder's last one closing fires `Away`.
    pub fn connection_closed(&self, browser: &BrowserId, now: SystemTime) {
        let mut state = self.lock_state();
        let Some(count) = state.open.get_mut(browser) else {
            return;
        };
        *count -= 1;
        if *count > 0 {
            return;
        }
        state.open.remove(browser);
        let away = match state.holder.as_mut() {
            Some(holder) if &holder.browser == browser && holder.away_since.is_none() => {
                holder.away_since = Some(now);
                true
            }
            _ => false,
        };
        drop(state);
        if away {
            self.fire(&AuthoringEvent::Away { since: now });
        }
    }

    /// Free authoring once the holder has been away for `GRACE` at `now`.
    pub fn tick(&self, now: SystemTime) {
        let mut state = self.lock_state();
        let expired = state
            .holder
            .as_ref()
            .and_then(|holder| holder.away_since)
            .and_then(|since| since.checked_add(GRACE))
            .is_some_and(|frees_at| frees_at <= now);
        if !expired {
            return;
        }
        state.holder = None;
        drop(state);
        self.fire(&AuthoringEvent::Freed {
            reason: FreeReason::GraceExpired,
        });
    }

    fn fire(&self, event: &AuthoringEvent) {
        let subscribers = self.lock_subscribers().clone();
        for subscriber in &subscribers {
            subscriber(event);
        }
    }

    fn lock_state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn lock_subscribers(&self) -> MutexGuard<'_, Vec<Subscriber>> {
        self.subscribers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

/// RFC 3339 UTC at whole seconds, the wire form of every authoring time.
fn wire_time(time: SystemTime) -> String {
    DateTime::<Utc>::from(time).to_rfc3339_opts(SecondsFormat::Secs, true)
}
