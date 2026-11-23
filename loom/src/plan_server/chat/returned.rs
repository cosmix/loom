//! Input a restart returns: texts the last server accepted and never saw confirmed
//! (`BridgeEvent::InputReturned`) go back to the browser that held authoring then, as one
//! `queue_returned` frame with the cause `recovery`. Authoring is memory only and a restart
//! frees it, so two runtime files carry the state across: `chat-holder.json` (who held
//! authoring last; a free keeps it) and `returned-input.json` (what waits for that browser).
//! Both name a browser by the hash of its cookie, never the cookie itself.
//!
//! Lock order: `ChatState::returned`, then the hub (the hub calls no hook while it holds its
//! lock).

use std::os::fd::AsRawFd;
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::{lock, ChatState};
use crate::fs::safe_fs::{safe_open_dirfd, safe_remove_in_workdir};
use crate::fs::safe_read::{is_not_found, read_bounded};
use crate::fs::safe_replace::replace_private;
use crate::plan_server::authoring::AuthoringEvent;
use crate::plan_server::context::PlanContext;
use crate::plan_server::ids::BrowserId;
use crate::plan_server::wire::chat::{ChatFrame, QueueReturnCause};
use crate::plan_server::wire::message::PlanMessage;

const HOLDER_FILE: &str = "chat-holder.json";
pub(super) const RETURNED_FILE: &str = "returned-input.json";
const MAX_BYTES: usize = 8 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
struct HolderFile {
    holder: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Entry {
    /// The tag of the browser the texts wait for. `None`: no holder file was readable at the
    /// restart, so the first tab that opens gets them (the accepted limit).
    pub(super) holder: Option<String>,
    pub(super) texts: Vec<String>,
}

#[derive(Default, Serialize, Deserialize)]
struct EntriesFile {
    entries: Vec<Entry>,
}

/// The returned input of this server run.
#[derive(Default)]
pub(super) struct Returned {
    /// The tag of the browser that held authoring when the last server stopped.
    pub(super) prior: Option<String>,
    /// That browser, once one of its tabs opened.
    pub(super) browser: Option<BrowserId>,
    pub(super) entries: Vec<Entry>,
}

pub(super) fn tag(browser: &BrowserId) -> String {
    crate::utils::sha256_hex(browser.as_str().as_bytes())
}

fn read_file<T: for<'de> Deserialize<'de>>(ctx: &PlanContext, name: &str) -> Option<T> {
    let dir = &ctx.runtime().dir;
    match read_bounded(dir, Path::new(name), MAX_BYTES) {
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(value) => Some(value),
            Err(error) => {
                tracing::warn!(file = name, %error, "cannot parse the returned-input state");
                None
            }
        },
        Err(error) if is_not_found(&error) => None,
        Err(error) => {
            tracing::warn!(file = name, %error, "cannot read the returned-input state");
            None
        }
    }
}

fn write_file(ctx: &PlanContext, name: &str, bytes: &[u8]) {
    let dir = &ctx.runtime().dir;
    if let Err(error) = replace_private(dir, Path::new(name), bytes) {
        tracing::warn!(file = name, %error, "cannot save the returned-input state");
    }
}

fn remove_file(ctx: &PlanContext, name: &str) {
    let dir = &ctx.runtime().dir;
    let removed =
        safe_open_dirfd(dir).and_then(|fd| safe_remove_in_workdir(fd.as_raw_fd(), Path::new(name)));
    if let Err(error) = removed {
        if !is_not_found(&error) {
            tracing::warn!(file = name, %error, "cannot remove the returned-input state");
        }
    }
}

/// What the last server left; read before any hook or recovery exists.
pub(super) fn load(ctx: &PlanContext) -> Returned {
    let prior = read_file::<HolderFile>(ctx, HOLDER_FILE).map(|file| file.holder);
    let entries = read_file::<EntriesFile>(ctx, RETURNED_FILE).map(|file| file.entries);
    Returned {
        prior,
        browser: None,
        entries: entries.unwrap_or_default(),
    }
}

/// Keep `chat-holder.json` naming whoever held authoring last (overwritten on each take).
pub(super) fn follow_holder(ctx: &Arc<PlanContext>) {
    let weak = Arc::downgrade(ctx);
    ctx.authoring().subscribe(Box::new(move |event| {
        let Some(ctx) = weak.upgrade() else {
            return;
        };
        match event {
            AuthoringEvent::Taken { .. } => {
                let Some(browser) = ctx.authoring().holder_browser() else {
                    return;
                };
                let file = HolderFile {
                    holder: tag(&browser),
                };
                match serde_json::to_vec(&file) {
                    Ok(bytes) => write_file(&ctx, HOLDER_FILE, &bytes),
                    Err(error) => tracing::warn!(%error, "cannot encode the chat holder"),
                }
            }
            // The file keeps naming the last holder after authoring is freed: a Failed chat
            // frees authoring while its queued input is still to be returned, and a restart
            // then must hand that input to the same browser, not to whichever tab opens first.
            AuthoringEvent::Freed { .. } | AuthoringEvent::Away { .. } | AuthoringEvent::Back => {}
        }
    }));
}

/// Write the waiting entries, or remove the file when none wait. Entries are persisted when
/// they arrive and again after their frame is handed to the hub: a failed write can deliver
/// twice, never lose.
fn persist(ctx: &PlanContext, entries: &[Entry]) {
    if entries.is_empty() {
        remove_file(ctx, RETURNED_FILE);
        return;
    }
    let file = EntriesFile {
        entries: entries.to_vec(),
    };
    match serde_json::to_vec(&file) {
        Ok(bytes) => write_file(ctx, RETURNED_FILE, &bytes),
        Err(error) => tracing::warn!(%error, "cannot encode the returned input"),
    }
}

/// Remove the entries `wanted` selects and return their texts in order.
fn drain(entries: &mut Vec<Entry>, wanted: impl Fn(&Option<String>) -> bool) -> Vec<String> {
    let all = std::mem::take(entries);
    let (taken, kept): (Vec<Entry>, Vec<Entry>) =
        all.into_iter().partition(|entry| wanted(&entry.holder));
    *entries = kept;
    taken.into_iter().flat_map(|entry| entry.texts).collect()
}

fn frame(texts: Vec<String>) -> PlanMessage {
    PlanMessage::Chat(Box::new(ChatFrame::QueueReturned {
        texts,
        cause: Some(QueueReturnCause::Recovery),
    }))
}

/// Recovery returned `texts`: keep them for the prior holder and hand them over now when one of
/// its tabs is open.
pub(super) fn on_returned(ctx: &PlanContext, state: &ChatState, texts: Vec<String>) {
    let mut returned = lock(&state.returned);
    let holder = returned.prior.clone();
    returned.entries.push(Entry {
        holder: holder.clone(),
        texts,
    });
    persist(ctx, &returned.entries);
    let open = returned
        .browser
        .clone()
        .filter(|browser| ctx.hub().connections_of(browser) > 0);
    let Some(browser) = open else {
        return;
    };
    let texts = drain(&mut returned.entries, |entry| *entry == holder);
    // The frame goes first: a crash between the send and this write delivers twice, never loses.
    ctx.hub().send_to_browser(&browser, &frame(texts));
    persist(ctx, &returned.entries);
}

/// A tab of `browser` opens: the frame carrying what waits for it, after the snapshot.
pub(super) fn on_open(
    ctx: &PlanContext,
    state: &ChatState,
    browser: &BrowserId,
) -> Option<PlanMessage> {
    let mut returned = lock(&state.returned);
    let own = tag(browser);
    let prior = returned.prior.as_deref() == Some(own.as_str());
    if prior {
        returned.browser = Some(browser.clone());
    }
    let texts = drain(&mut returned.entries, |holder| match holder {
        Some(waiting) => *waiting == own,
        None => true,
    });
    if texts.is_empty() {
        return None;
    }
    persist(ctx, &returned.entries);
    Some(frame(texts))
}
