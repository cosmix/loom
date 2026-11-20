//! Kickoff drafts that outlive their session: `<runtime>/kickoffs/<session id>.json`.
//!
//! A draft is written when a kickoff starts and rewritten on every change of what the user
//! typed, what the agent proposed, the user's rename, the confirmed target and the session's
//! end. Every read-modify-write holds `ChatState::drafts`: the bridge listener, the dispatch
//! thread and the `Ended` handler all write. A draft lands, and is deleted on the next listing
//! or write, only when its session finished: the agent's done fence ended it (`finished`) and
//! the confirmed document exists, or a locked spelling of that document exists. A draft whose
//! session was set aside, interrupted or lost stays listed even when its document exists, so it
//! can be continued or deleted.

mod delete;
#[cfg(test)]
mod delete_tests;
mod encode;
mod handlers;
mod store;

use std::path::Path;
use std::sync::MutexGuard;

use anyhow::Result;
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

pub(super) use self::delete::{remove_written, target_size, written};
use self::store::Files;
use super::{lock, ChatState};
use crate::fs::safe_fs::{safe_create_dir_all, safe_replace_outside_workdir};
use crate::models::dispute::truncate_to_byte_limit;
use crate::plan_server::context::PlanContext;
use crate::plan_server::request::{ApiRequest, ApiResponse};
use crate::plan_server::routes::{BodyKind, Method, Role, RouteSpec};
use crate::plan_server::uploads;
use crate::plan_server::wire::kickoff::{KickoffDraftView, KickoffShape};
use crate::plan_server::wire::message::PlanMessage;
use crate::plan_session::attachments::{AttachmentInput, MAX_ATTACHMENTS};
use crate::plan_session::bridge::Bridge;
use crate::plan_session::scope::KickoffSize;
use crate::planning::paths::check_relpath_syntax;

/// The directory of the draft files, inside the runtime directory.
const DIR: &str = "kickoffs";

/// The listing route.
const PATH: &str = "/api/kickoffs";

/// The prefix of one draft's route, which ends in the session id.
const DETAIL_PREFIX: &str = "/api/kickoffs/";

/// A draft keeps this many messages, the first and the newest.
pub(super) const MAX_MESSAGES: usize = 40;

/// A message longer than this is cut, at a char boundary, with [`CUT`].
pub(super) const MAX_MESSAGE_BYTES: usize = 4096;

/// A draft file larger than this is not read, so none is written larger.
pub(super) const MAX_FILE_BYTES: usize = 256 * 1024;

const CUT: &str = " […]";

/// The stored form of one kickoff.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct Draft {
    pub(super) session: String,
    pub(super) started_at: String,
    pub(super) updated_at: String,
    /// The size the kickoff was started with, from its offer; the agent's block never changes it.
    pub(super) size: KickoffSize,
    /// Whether the session ended because the agent's done fence asked.
    #[serde(default)]
    pub(super) finished: bool,
    /// The texts the user typed, oldest first.
    #[serde(default)]
    pub(super) messages: Vec<String>,
    /// The agent's last shape.
    #[serde(default)]
    pub(super) shape: Option<KickoffShape>,
    /// The start command's attachments, as the browser sent them.
    #[serde(default)]
    pub(super) attachments: Vec<AttachmentInput>,
    /// The confirmed document's repository path.
    #[serde(default)]
    pub(super) target: Option<String>,
    /// Whether the session was running when this was written; never read alone (see [`views`]).
    pub(super) live: bool,
}

/// What a new draft starts from: nothing but the typed text, or an earlier draft's content.
pub(super) struct Seed {
    pub(super) size: KickoffSize,
    pub(super) messages: Vec<String>,
    pub(super) attachments: Vec<AttachmentInput>,
    pub(super) shape: Option<KickoffShape>,
}

/// A lowercase hyphenated uuid: the id becomes a file name.
pub(super) fn canonical(id: &str) -> bool {
    uuid::Uuid::try_parse(id).is_ok_and(|uuid| uuid.hyphenated().to_string() == id)
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

impl Draft {
    fn started(session: &str, seed: Seed) -> Draft {
        let at = now();
        let mut draft = Draft {
            session: session.to_owned(),
            started_at: at.clone(),
            updated_at: at,
            size: seed.size,
            finished: false,
            messages: Vec::new(),
            shape: seed.shape,
            attachments: seed.attachments,
            target: None,
            live: true,
        };
        draft.attachments.truncate(MAX_ATTACHMENTS);
        for message in &seed.messages {
            draft.push_message(message);
        }
        draft
    }

    /// Append what the user typed: cut to the byte cap, and the oldest messages after the first
    /// dropped beyond the count cap. A blank text is not recorded.
    pub(super) fn push_message(&mut self, text: &str) {
        if text.trim().is_empty() {
            return;
        }
        self.messages.push(cut(text));
        while self.messages.len() > MAX_MESSAGES {
            self.messages.remove(1);
        }
    }

    /// The agent's name for the work.
    pub(super) fn name(&self) -> Option<&str> {
        self.shape.as_ref().and_then(|shape| shape.name.as_deref())
    }

    fn view(&self, live: bool) -> KickoffDraftView {
        let shape = self.shape.as_ref();
        KickoffDraftView {
            session: self.session.clone(),
            name: self.name().map(str::to_owned),
            size: self.size,
            problem: shape
                .map(|shape| shape.problem.clone())
                .filter(|problem| !problem.is_empty()),
            started_at: self.started_at.clone(),
            updated_at: self.updated_at.clone(),
            live,
        }
    }

    pub(super) fn updated(&self) -> DateTime<Utc> {
        crate::utils::parse_rfc3339_utc(&self.updated_at).unwrap_or(DateTime::UNIX_EPOCH)
    }

    /// The limits a draft this server wrote keeps; a file that breaks one is not shown.
    pub(super) fn within_caps(&self) -> bool {
        let shape_ok = self
            .shape
            .as_ref()
            .is_none_or(super::kickoff::within_limits);
        let target_ok = self
            .target
            .as_deref()
            .is_none_or(|target| check_relpath_syntax(target).is_ok());
        self.messages.len() <= MAX_MESSAGES
            && self
                .messages
                .iter()
                .all(|text| text.len() <= MAX_MESSAGE_BYTES)
            && self.attachments.len() <= MAX_ATTACHMENTS
            && crate::utils::parse_rfc3339_utc(&self.started_at).is_some()
            && crate::utils::parse_rfc3339_utc(&self.updated_at).is_some()
            && shape_ok
            && target_ok
    }
}

/// `text` within [`MAX_MESSAGE_BYTES`], cut at a char boundary and marked.
fn cut(text: &str) -> String {
    if text.len() <= MAX_MESSAGE_BYTES {
        return text.to_owned();
    }
    format!(
        "{}{CUT}",
        truncate_to_byte_limit(text, MAX_MESSAGE_BYTES - CUT.len())
    )
}

fn drafts_lock(state: &ChatState) -> MutexGuard<'_, ()> {
    lock(&state.drafts)
}

/// Write `draft` and refresh the stored time of every uploaded file it names, so the expiry
/// sweep never removes a file an unfinished draft still references.
fn save(ctx: &PlanContext, files: &Files, draft: &Draft) -> Result<()> {
    let runtime = &ctx.runtime().dir;
    safe_create_dir_all(runtime, Path::new(DIR), 0o700)?;
    let bytes = encode::within_cap(draft)?;
    safe_replace_outside_workdir(&files.path(&draft.session), runtime, &bytes)?;
    for input in &draft.attachments {
        let AttachmentInput::File(file) = input else {
            continue;
        };
        if let Err(error) = uploads::touch(runtime, &file.id) {
            tracing::warn!(file = %file.id, "cannot refresh an attachment's age: {error}");
        }
    }
    Ok(())
}

/// Write the first file of `session`, then tell every browser. The file is flagged live only
/// while `session` is the bridge's running chat: the driver launches asynchronously, so its
/// `Ended` can arrive before this file exists, and that event finds nothing to mark ended.
/// Lock order is the drafts mutex, then the bridge; nothing takes them the other way.
pub(super) fn create(
    ctx: &PlanContext,
    state: &ChatState,
    session: &str,
    seed: Seed,
) -> Result<(), String> {
    if !canonical(session) {
        return Err(format!("not a session id: {session}"));
    }
    let mut draft = Draft::started(session, seed);
    let saved = {
        let _held = drafts_lock(state);
        draft.live = running(ctx).as_deref() == Some(session);
        save(ctx, &Files::new(ctx), &draft)
    };
    saved.map_err(|error| format!("cannot store the draft: {error:#}"))?;
    publish(ctx, state);
    Ok(())
}

/// Apply `change` to the stored draft of `session` and write it back; false when there is no
/// such draft (never started, discarded or landed) or it could not be written. A draft that
/// lands through `change` is removed and counts as changed: it returns true and publishes.
pub(super) fn update(
    ctx: &PlanContext,
    state: &ChatState,
    session: &str,
    change: impl FnOnce(&mut Draft),
) -> bool {
    let files = Files::new(ctx);
    let changed = {
        let _held = drafts_lock(state);
        let Some(mut draft) = files.load(session) else {
            return false;
        };
        change(&mut draft);
        draft.updated_at = now();
        if files.landed(&draft) {
            files.remove(session)
        } else if let Err(error) = save(ctx, &files, &draft) {
            tracing::warn!(session, "kickoff draft not written: {error:#}");
            false
        } else {
            true
        }
    };
    if changed {
        publish(ctx, state);
    }
    changed
}

/// The stored draft of `session`, unless it has landed (then it is deleted).
pub(super) fn get(ctx: &PlanContext, state: &ChatState, session: &str) -> Option<Draft> {
    let files = Files::new(ctx);
    {
        let _held = drafts_lock(state);
        let draft = files.load(session)?;
        if !files.landed(&draft) {
            return Some(draft);
        }
        files.remove(session);
    }
    // Other tabs still list the landed draft on their shelf until they are told.
    publish(ctx, state);
    None
}

/// Delete the draft of `session`.
pub(super) fn remove(ctx: &PlanContext, state: &ChatState, session: &str) -> bool {
    let removed = {
        let _held = drafts_lock(state);
        Files::new(ctx).remove(session)
    };
    if removed {
        publish(ctx, state);
    }
    removed
}

/// The session of another stored draft that holds `target` as its confirmed document, if any.
pub(super) fn holder_of(
    ctx: &PlanContext,
    state: &ChatState,
    target: &str,
    except: &str,
) -> Option<String> {
    let held = |draft: &Draft| draft.session != except && draft.target.as_deref() == Some(target);
    stored(ctx, state)
        .into_iter()
        .find(held)
        .map(|draft| draft.session)
}

/// The session that is the running chat, if any.
pub(super) fn running(ctx: &PlanContext) -> Option<String> {
    ctx.extensions().get::<Bridge>()?.chat_session_id()
}

/// Every draft that has not landed, newest first.
fn stored(ctx: &PlanContext, state: &ChatState) -> Vec<Draft> {
    let files = Files::new(ctx);
    let _held = drafts_lock(state);
    let mut drafts: Vec<Draft> = files
        .sessions()
        .iter()
        .filter_map(|session| files.load(session))
        .filter(|draft| {
            let landed = files.landed(draft);
            if landed {
                files.remove(&draft.session);
            }
            !landed
        })
        .collect();
    drafts.sort_by(|a, b| (b.updated(), &b.session).cmp(&(a.updated(), &a.session)));
    drafts
}

/// The views of `drafts`: live only when the file says so and that session is the running chat.
fn views(ctx: &PlanContext, drafts: &[Draft]) -> Vec<KickoffDraftView> {
    let running = running(ctx);
    let live = |draft: &Draft| draft.live && running.as_deref() == Some(draft.session.as_str());
    drafts.iter().map(|draft| draft.view(live(draft))).collect()
}

pub(super) fn list(ctx: &PlanContext, state: &ChatState) -> Vec<KickoffDraftView> {
    views(ctx, &stored(ctx, state))
}

/// What a browser is told on opening a tab and after every change.
pub(super) fn snapshot(ctx: &PlanContext, state: &ChatState) -> PlanMessage {
    PlanMessage::KickoffDrafts {
        drafts: list(ctx, state),
    }
}

pub(super) fn publish(ctx: &PlanContext, state: &ChatState) {
    ctx.hub().broadcast(&snapshot(ctx, state));
}

/// Whether `draft` is the running chat.
pub(super) fn is_live(ctx: &PlanContext, draft: &Draft) -> bool {
    draft.live && running(ctx).as_deref() == Some(draft.session.as_str())
}

/// Remove the drafts untouched for 30 days, once per server start.
pub(super) fn sweep(ctx: &PlanContext) {
    let removed = Files::new(ctx).sweep(Utc::now());
    if removed > 0 {
        tracing::info!("kickoff drafts: removed {removed} expired drafts");
    }
}

/// The listing and one draft's detail.
pub fn routes() -> Vec<RouteSpec> {
    let route = |path: &'static str, handler: fn(&ApiRequest<'_>) -> ApiResponse| RouteSpec {
        method: Method::Get,
        path,
        body: BodyKind::None,
        role: Role::Viewer,
        handler,
    };
    // A path ending in `/` is a prefix, which makes the second route the one-draft lookup.
    vec![
        route(PATH, handlers::listing),
        route(DETAIL_PREFIX, handlers::detail),
    ]
}
