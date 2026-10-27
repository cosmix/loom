//! Presence of connected browsers: public ids, display names, open tabs and what each reads.
//!
//! Every browser that presents its cookie (opens a tab, names itself, calls the API) stays
//! recorded for the life of the process so its public id and "Viewer N" number never change.
//! Only [`Presence::public_id`], [`Presence::set_name`] and [`Presence::opened`] record. A
//! cookie-less HTTP request carries a browser id the gate just minted; the router never passes
//! it here, so the map is bounded by the browsers that send their cookie back.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::plan_server::authoring::Authoring;
use crate::plan_server::ids::{BrowserId, ConnCtx, ConnId, PublicId};
use crate::plan_server::wire::paths::to_doc_rel;
use crate::plan_server::wire::presence::{PresenceEntry, PresenceList};
use crate::planning::paths::check_relpath_syntax;

/// Longest display name kept, in `char`s.
const MAX_NAME_CHARS: usize = 32;

/// Unicode general category Cf (format), inclusive ranges, from the Unicode Character Database
/// `extracted/DerivedGeneralCategory.txt` (Unicode 15.1). Covers the bidirectional controls
/// U+061C, U+200E, U+200F, U+202A..U+202E and U+2066..U+2069.
const FORMAT_CHARS: &[(u32, u32)] = &[
    (0x00AD, 0x00AD),
    (0x0600, 0x0605),
    (0x061C, 0x061C),
    (0x06DD, 0x06DD),
    (0x070F, 0x070F),
    (0x0890, 0x0891),
    (0x08E2, 0x08E2),
    (0x180E, 0x180E),
    (0x200B, 0x200F),
    (0x202A, 0x202E),
    (0x2060, 0x2064),
    (0x2066, 0x206F),
    (0xFEFF, 0xFEFF),
    (0xFFF9, 0xFFFB),
    (0x110BD, 0x110BD),
    (0x110CD, 0x110CD),
    (0x13430, 0x1343F),
    (0x1BCA0, 0x1BCA3),
    (0x1D173, 0x1D17A),
    (0xE0001, 0xE0001),
    (0xE0020, 0xE007F),
];

/// Drop every control (Cc) and format (Cf) character, trim, and keep at most 32 `char`s.
pub fn sanitize_name(raw: &str) -> String {
    let kept: String = raw
        .chars()
        .filter(|&c| !c.is_control() && !is_format(c))
        .collect();
    let truncated: String = kept.trim().chars().take(MAX_NAME_CHARS).collect();
    truncated.trim_end().to_owned()
}

fn is_format(c: char) -> bool {
    let code = u32::from(c);
    FORMAT_CHARS
        .iter()
        .any(|&(first, last)| (first..=last).contains(&code))
}

/// A `reading` path is repo-relative under `doc/` and passes the request path syntax check.
fn valid_reading(path: &str) -> bool {
    to_doc_rel(path).is_some_and(|rel| check_relpath_syntax(rel).is_ok())
}

struct BrowserEntry {
    public_id: PublicId,
    viewer: u32,
    /// Sanitized; empty when unnamed.
    name: String,
    /// Order in which the current name was set; the earlier of two equal names keeps it.
    named_at: u64,
    tabs: u32,
}

struct ConnEntry {
    browser: BrowserId,
    reading: Option<String>,
    /// Order in which `reading` was last set; a browser shows its most recent tab's reading.
    read_at: u64,
}

#[derive(Default)]
struct State {
    browsers: HashMap<BrowserId, BrowserEntry>,
    conns: HashMap<ConnId, ConnEntry>,
    next_viewer: u32,
    next_seq: u64,
}

impl State {
    fn browser(&mut self, browser: &BrowserId, minted: Option<&PublicId>) -> &mut BrowserEntry {
        let next_viewer = &mut self.next_viewer;
        self.browsers.entry(browser.clone()).or_insert_with(|| {
            *next_viewer += 1;
            BrowserEntry {
                public_id: minted.cloned().unwrap_or_else(PublicId::mint),
                viewer: *next_viewer,
                name: String::new(),
                named_at: 0,
                tabs: 0,
            }
        })
    }

    fn seq(&mut self) -> u64 {
        self.next_seq += 1;
        self.next_seq
    }

    /// The shown name: "Viewer N" when unnamed, or when a connected browser set the same name
    /// first.
    fn display_name(&self, browser: &BrowserId) -> String {
        let Some(entry) = self.browsers.get(browser) else {
            return String::new();
        };
        let taken = self.browsers.iter().any(|(other, them)| {
            other != browser
                && them.tabs > 0
                && them.name == entry.name
                && them.named_at < entry.named_at
        });
        if entry.name.is_empty() || taken {
            format!("Viewer {}", entry.viewer)
        } else {
            entry.name.clone()
        }
    }

    fn reading_of(&self, browser: &BrowserId) -> Option<String> {
        self.conns
            .values()
            .filter(|conn| &conn.browser == browser && conn.reading.is_some())
            .max_by_key(|conn| conn.read_at)
            .and_then(|conn| conn.reading.clone())
    }
}

/// Connected browsers of one planning server. `Send + Sync`; share it by reference.
#[derive(Default)]
pub struct Presence {
    state: Mutex<State>,
}

impl Presence {
    pub fn new() -> Self {
        Self::default()
    }

    /// The browser's public id, minted on first sight and stable for the process.
    pub fn public_id(&self, browser: &BrowserId) -> PublicId {
        self.lock().browser(browser, None).public_id.clone()
    }

    /// Record the browser's display name after `sanitize_name`.
    pub fn set_name(&self, browser: &BrowserId, raw: &str) {
        let name = sanitize_name(raw);
        let mut state = self.lock();
        if state.browser(browser, None).name == name {
            return;
        }
        let seq = state.seq();
        let entry = state.browser(browser, None);
        entry.name = name;
        entry.named_at = seq;
    }

    /// The name other browsers see: "Viewer N" when unnamed or a duplicate, empty for a browser
    /// never recorded. Records nothing.
    pub fn name_of(&self, browser: &BrowserId) -> String {
        self.lock().display_name(browser)
    }

    /// Record what `conn` reads; a path that is not a valid `doc/...` path is stored as `None`.
    pub fn set_reading(&self, conn: ConnId, path: Option<String>) {
        let reading = path.filter(|path| valid_reading(path));
        let mut state = self.lock();
        let seq = state.seq();
        if let Some(entry) = state.conns.get_mut(&conn) {
            entry.reading = reading;
            entry.read_at = seq;
        }
    }

    pub fn opened(&self, conn: &ConnCtx) {
        let mut state = self.lock();
        state.browser(&conn.browser, Some(&conn.public_id)).tabs += 1;
        state.conns.insert(
            conn.id,
            ConnEntry {
                browser: conn.browser.clone(),
                reading: None,
                read_at: 0,
            },
        );
    }

    pub fn closed(&self, conn: ConnId) {
        let mut state = self.lock();
        let Some(entry) = state.conns.remove(&conn) else {
            return;
        };
        if let Some(browser) = state.browsers.get_mut(&entry.browser) {
            browser.tabs = browser.tabs.saturating_sub(1);
        }
    }

    /// Browsers with at least one open tab, plus the holder while it has none, in first-seen
    /// order.
    pub fn list(&self, authoring: &Authoring) -> PresenceList {
        // Read the holder before locking presence: the two locks are never held together.
        let holder = authoring.holder();
        let state = self.lock();
        let mut shown: Vec<(&BrowserId, &BrowserEntry)> = state
            .browsers
            .iter()
            .filter(|(_, entry)| {
                let holds = holder
                    .as_ref()
                    .is_some_and(|(id, _)| id == &entry.public_id);
                entry.tabs > 0 || holds
            })
            .collect();
        shown.sort_by_key(|(_, entry)| entry.viewer);
        let entries = shown
            .into_iter()
            .map(|(browser, entry)| {
                let held = holder.as_ref().filter(|(id, _)| id == &entry.public_id);
                PresenceEntry {
                    public_id: entry.public_id.clone(),
                    name: state.display_name(browser),
                    holder: held.is_some(),
                    away: held.is_some_and(|(_, away_since)| away_since.is_some()),
                    reading: state.reading_of(browser),
                    tabs: entry.tabs,
                }
            })
            .collect();
        PresenceList { entries }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
