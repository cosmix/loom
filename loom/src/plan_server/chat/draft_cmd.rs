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
use crate::plan_session::bridge::{Bridge, BridgeError, EndCause, EndMode};
use crate::plan_session::input::sanitize;

const UNKNOWN: &str = "There is no such unfinished kickoff";
const WROTE_FILES: &str = "This draft wrote files; delete them too or keep the draft";

/// How long a confirmed kickoff's closing write may take once the holder sets it aside; the
/// server's 600 s bound is for an end nobody is watching. Revisit if a slow closing write loses
/// work the user wanted kept.
const SET_ASIDE_BOUND: Duration = Duration::from_secs(60);

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
/// interrupted, then given a bounded closing write.
pub(super) fn set_aside(ctx: &PlanContext, conn: &ConnCtx, session: &str, now: bool) -> Outcome {
    let state = chat_state(ctx)?;
    let draft = known_draft(ctx, &state, session)?;
    on_chat(ctx, conn, CLOSING, |bridge, id| {
        if id != draft.session {
            return Err(BridgeError::Refused("That kickoff is not open".to_owned()));
        }
        match now || draft.target.is_none() {
            true => bridge.end(id, EndMode::Teardown, EndCause::Holder),
            false => close_within_bound(bridge, id),
        }
    })
}

/// Stop the running turn so the closing message reaches an idle agent, then end with a final
/// write bounded by [`SET_ASIDE_BOUND`]. The texts the interrupt and the end take off the queue
/// both go back to the caller.
fn close_within_bound(bridge: &Bridge, id: &str) -> Result<Vec<String>, BridgeError> {
    let mut returned = match bridge.interrupt(id) {
        Ok(texts) => texts,
        // No turn is running: the closing write can go out at once.
        Err(BridgeError::NotRunning) => Vec::new(),
        Err(error) => {
            tracing::debug!(session = %id, %error, "set aside: the turn was not interrupted");
            Vec::new()
        }
    };
    let ended = bridge.end_within(id, EndMode::FinalWrite, EndCause::Holder, SET_ASIDE_BOUND)?;
    returned.extend(ended);
    Ok(returned)
}

/// `discard_kickoff`: delete the draft of an ended kickoff, and with `delete_files` the files its
/// confirmed target wrote. The files come from the stored target through `drafts`, never from
/// the request.
pub(super) fn discard(
    ctx: &PlanContext,
    conn: &ConnCtx,
    session: &str,
    delete_files: bool,
) -> Outcome {
    writable(ctx)?;
    let state = chat_state(ctx)?;
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
        match delete_files {
            true => {
                drafts::remove_written(ctx.root(), target)?;
            }
            false if wrote_files(ctx, target) => return Err(WROTE_FILES.to_owned()),
            false => {}
        }
    }
    drafts::remove(ctx, &state, session);
    Ok(Vec::new())
}

/// Whether the confirmed `target` has files on disk. A target that is no kickoff target
/// has none to protect: only the draft goes.
fn wrote_files(ctx: &PlanContext, target: &str) -> bool {
    drafts::written(ctx.root(), target).is_ok_and(|written| !written.paths.is_empty())
}
