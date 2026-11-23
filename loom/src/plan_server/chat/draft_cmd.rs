//! Kickoff drafts in the chat: what each command and event writes, and the holder commands that
//! set aside and discard one. Continuing one is `draft_continue`.

use std::time::Duration;

use super::dispatch::{chat_state, on_chat, refuse_new_work, Outcome, CLOSING};
use super::drafts::{self, Draft, Seed};
use super::team::offer_size;
use super::ChatState;
use crate::plan_server::authoring::{Refusal, Take};
use crate::plan_server::context::PlanContext;
use crate::plan_server::ids::ConnCtx;
use crate::plan_server::wire::kickoff::KickoffShape;
use crate::plan_session::attachments::{AttachmentInput, MAX_ATTACHMENTS};
use crate::plan_session::bridge::{BridgeError, EndCause, EndMode};
use crate::plan_session::input::sanitize;

const UNKNOWN: &str = "There is no such unfinished kickoff";
const WROTE_FILES: &str = "This draft wrote files; delete them too or keep the draft";
const NOT_REMOVED: &str = "The draft could not be deleted; try again";
/// A stored target of another size than its draft's: a hand-edited draft file.
pub(super) const SIZE_MISMATCH: &str = "This draft's target does not match its size";

/// How long a confirmed kickoff's closing write may take once the holder sets it aside; the
/// server's 600 s bound is for an end nobody is watching. Revisit if a slow closing write loses
/// work the user wanted kept.
pub(super) const SET_ASIDE_BOUND: Duration = Duration::from_secs(60);

/// What a kickoff start puts into its first draft: the typed text and the attachment inputs.
pub(super) struct StartDraft(Option<Seed>);

impl StartDraft {
    /// The draft of a start with `offer`, sized by it; nothing for any other start.
    pub(super) fn of(offer: Option<&str>, text: &str, inputs: &[AttachmentInput]) -> StartDraft {
        let Some(size) = offer_size(offer) else {
            return StartDraft(None);
        };
        let typed = sanitize(text);
        StartDraft(Some(Seed {
            messages: vec![typed],
            attachments: inputs.iter().take(MAX_ATTACHMENTS).cloned().collect(),
            shape: None,
            target: None,
            size,
        }))
    }

    /// Write the first file of `session`. The chat is running by now, so a write that fails is
    /// logged and the kickoff goes on without a draft.
    pub(super) fn record(self, ctx: &PlanContext, state: &ChatState, session: &str) {
        let Some(seed) = self.0 else {
            return;
        };
        if let Err(error) = drafts::create(ctx, state, session, seed) {
            tracing::warn!(session, "kickoff draft not started: {error}");
        }
    }
}

/// A user's send to `session`.
pub(super) fn on_send(ctx: &PlanContext, state: &ChatState, session: &str, text: &str) {
    let typed = sanitize(text);
    drafts::update(ctx, state, session, |draft| draft.push_message(&typed));
}

/// The agent's last shape of `session` changed.
pub(super) fn on_shape(ctx: &PlanContext, state: &ChatState, session: &str, shape: KickoffShape) {
    drafts::update(ctx, state, session, |draft| draft.shape = Some(shape));
}

/// The kickoff of `session` was confirmed for `target`.
pub(super) fn on_confirm(ctx: &PlanContext, state: &ChatState, session: &str, target: &str) {
    drafts::update(ctx, state, session, |draft| {
        draft.target = Some(target.to_owned());
    });
}

/// `session` ended: its draft stays, no longer live, and is finished only when the agent's done
/// fence asked for the end (a set aside, a crash or a restart leaves it unfinished). Every ended
/// session passes here, and only a kickoff has a file.
pub(super) fn ended(ctx: &PlanContext, state: &ChatState, session: &str, cause: Option<EndCause>) {
    drafts::update(ctx, state, session, |draft| {
        draft.live = false;
        draft.finished = cause == Some(EndCause::Agent);
    });
}

/// Refuse a command the stopping server or read-only mode does not take.
pub(super) fn writable(ctx: &PlanContext) -> Result<(), String> {
    refuse_new_work(ctx)?;
    match ctx.read_only().is_read_only() {
        true => Err(ctx.refusal_message(&Refusal::ReadOnlyMode)),
        false => Ok(()),
    }
}

pub(super) fn known_draft(
    ctx: &PlanContext,
    state: &ChatState,
    session: &str,
) -> Result<Draft, String> {
    if !drafts::canonical(session) {
        return Err(format!("{session:?} is not a kickoff session id"));
    }
    drafts::get(ctx, state, session).ok_or_else(|| UNKNOWN.to_owned())
}

/// `set_aside_kickoff`: end the running kickoff `session` and keep its draft. An unconfirmed
/// kickoff has nothing to write and `now` means stop at once: both tear down. A confirmed one is
/// interrupted (unless its closing turn already runs), then given a write bounded by
/// [`SET_ASIDE_BOUND`].
pub(super) fn set_aside(ctx: &PlanContext, conn: &ConnCtx, session: &str, now: bool) -> Outcome {
    let state = chat_state(ctx)?;
    let draft = known_draft(ctx, &state, session)?;
    on_chat(ctx, conn, CLOSING, |bridge, id| {
        if id != draft.session {
            return Err(BridgeError::Refused("That kickoff is not open".to_owned()));
        }
        match now || draft.target.is_none() {
            true => bridge.end(id, EndMode::Teardown, EndCause::Holder),
            false => bridge.close_within(id, EndCause::Holder, SET_ASIDE_BOUND),
        }
    })
}

/// `discard_kickoff`: delete the draft of an ended kickoff, and with `delete_files` the files its
/// confirmed target wrote. The files come from the stored target through `drafts`, never from
/// the request. Holding the start lock orders it against a continuation of the same draft:
/// whichever runs second reads the draft as it is then.
pub(super) fn discard(
    ctx: &PlanContext,
    conn: &ConnCtx,
    session: &str,
    delete_files: bool,
) -> Outcome {
    writable(ctx)?;
    let state = chat_state(ctx)?;
    let _starting = state.starting();
    // Nobody holds authoring once a chat has ended; another browser's hold still refuses.
    match ctx.require_authoring(&conn.browser, Take::HolderOnly) {
        Ok(()) | Err(Refusal::NoHolder) => {}
        Err(refusal) => return Err(ctx.refusal_message(&refusal)),
    }
    let draft = known_draft(ctx, &state, session)?;
    if drafts::is_live(ctx, &draft) {
        return Err("Set it aside first".to_owned());
    }
    if let Some(target) = draft.target.as_deref() {
        discard_files(ctx, &draft, target, delete_files)?;
    }
    match drafts::remove(ctx, &state, session) {
        true => Ok(Vec::new()),
        false => Err(NOT_REMOVED.to_owned()),
    }
}

/// The file half of a discard of `draft`, whose confirmed document is `target`. A target of
/// another size than the draft's (a hand-edited draft file) names no files of this draft: they
/// are never deleted, and keeping them lets the draft go.
fn discard_files(
    ctx: &PlanContext,
    draft: &Draft,
    target: &str,
    delete_files: bool,
) -> Result<(), String> {
    let foreign = drafts::target_size(target).is_ok_and(|size| size != draft.size);
    match (delete_files, foreign) {
        (true, true) => Err(SIZE_MISMATCH.to_owned()),
        (true, false) => drafts::remove_written(ctx.root(), target).map(drop),
        (false, false) if wrote_files(ctx, target) => Err(WROTE_FILES.to_owned()),
        (false, _) => Ok(()),
    }
}

/// Whether the confirmed `target` has files on disk. A target that is no kickoff target has
/// none to protect, and an I/O refusal while looking reads as none too: the safe direction,
/// since only the draft goes and every file stays.
fn wrote_files(ctx: &PlanContext, target: &str) -> bool {
    drafts::written(ctx.root(), target).is_ok_and(|written| !written.paths.is_empty())
}
