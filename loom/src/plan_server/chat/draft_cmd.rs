//! Kickoff drafts in the chat: what each command and event writes, and the holder commands that
//! rename, set aside, continue and discard one.

use std::sync::Arc;

use super::dispatch::{
    bridge, chat_state, checked, launch, on_chat, refuse_new_work, remember_holder, take_authoring,
    ChatOwner, Outcome, CLOSING, NO_CHAT,
};
use super::drafts::{self, Draft, Seed};
use super::release::give_back;
use super::{lock, ChatState};
use crate::plan_server::authoring::{Refusal, Take};
use crate::plan_server::context::PlanContext;
use crate::plan_server::ids::ConnCtx;
use crate::plan_server::wire::kickoff::{KickoffShape, KickoffView};
use crate::plan_server::wire::message::PlanMessage;
use crate::plan_session::attachments::{resolve_input, AttachmentInput, MAX_ATTACHMENTS};
use crate::plan_session::bridge::{Bridge, BridgeError, EndCause, EndMode};
use crate::plan_session::input::sanitize;
use crate::plan_session::scope::{ArmRefusal, KickoffSize};
use crate::planning::guidance::{continuation_message, DraftContinuation};
use crate::planning::kinds::is_feature_name;

const UNKNOWN: &str = "There is no such unfinished kickoff";

/// What a kickoff start puts into its first draft: the typed text and the attachment inputs.
pub(super) struct StartDraft(Option<Seed>);

impl StartDraft {
    /// The draft of a start with `offer`; nothing for any other start.
    pub(super) fn of(offer: Option<&str>, text: &str, inputs: &[AttachmentInput]) -> StartDraft {
        if !matches!(offer, Some("kickoff" | "kickoff-small")) {
            return StartDraft(None);
        }
        let typed = sanitize(text);
        StartDraft(Some(Seed {
            messages: vec![typed],
            attachments: inputs.iter().take(MAX_ATTACHMENTS).cloned().collect(),
            ..Seed::default()
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

/// `session` ended: its draft stays, no longer live. Every ended session passes here, and only a
/// kickoff has a file.
pub(super) fn ended(ctx: &PlanContext, state: &ChatState, session: &str) {
    drafts::update(ctx, state, session, |draft| draft.live = false);
}

/// Refuse a command the stopping server or read-only mode does not take.
fn writable(ctx: &PlanContext) -> Result<(), String> {
    refuse_new_work(ctx)?;
    match ctx.read_only().is_read_only() {
        true => Err(ctx.refusal_message(&Refusal::ReadOnlyMode)),
        false => Ok(()),
    }
}

fn known_draft(ctx: &PlanContext, state: &ChatState, session: &str) -> Result<Draft, String> {
    if !drafts::canonical(session) {
        return Err(format!("{session:?} is not a kickoff session id"));
    }
    drafts::get(ctx, state, session).ok_or_else(|| UNKNOWN.to_owned())
}

fn announce(ctx: &PlanContext, view: KickoffView) {
    ctx.hub().broadcast(&PlanMessage::Kickoff(Box::new(view)));
}

/// `rename_kickoff`: the user's name for the running kickoff's work replaces the agent's.
pub(super) fn rename(ctx: &PlanContext, conn: &ConnCtx, name: &str) -> Outcome {
    writable(ctx)?;
    let (bridge, state) = (bridge(ctx)?, chat_state(ctx)?);
    // Serialised with a confirmation, which arms the write grant for the name it was given.
    let _starting = state.starting();
    ctx.require_authoring(&conn.browser, Take::HolderOnly)
        .map_err(|refusal| ctx.refusal_message(&refusal))?;
    remember_holder(ctx, conn);
    let id = bridge.chat_session_id().ok_or_else(|| NO_CHAT.to_owned())?;
    if !is_feature_name(name) {
        return Err(ArmRefusal::InvalidName(name.to_owned()).to_string());
    }
    lock(&state.kickoff).renamable(&id).map_err(str::to_owned)?;
    let message = checked(ctx, &rename_message(name), Vec::new())?;
    bridge
        .send(&id, message.formatted, message.attachments)
        .map_err(|error| error.to_string())?;
    let view = lock(&state.kickoff).rename(&id, name);
    let view = view.map_err(str::to_owned)?;
    drafts::update(ctx, &state, &id, |draft| {
        draft.name_override = Some(name.to_owned());
    });
    announce(ctx, view);
    Ok(Vec::new())
}

fn rename_message(name: &str) -> String {
    format!("I renamed this work to `{name}`. Use that name from now on.")
}

/// `set_aside_kickoff`: end the running kickoff `session` and keep its draft.
pub(super) fn set_aside(ctx: &PlanContext, conn: &ConnCtx, session: &str) -> Outcome {
    let state = chat_state(ctx)?;
    let draft = known_draft(ctx, &state, session)?;
    on_chat(ctx, conn, CLOSING, |bridge, id| {
        if id != draft.session {
            return Err(BridgeError::Refused("That kickoff is not open".to_owned()));
        }
        bridge.end(id, EndMode::FinalWrite, EndCause::Holder)
    })
}

/// `discard_kickoff`: delete the draft of an ended kickoff.
pub(super) fn discard(ctx: &PlanContext, conn: &ConnCtx, session: &str) -> Outcome {
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
    drafts::remove(ctx, &state, session);
    Ok(Vec::new())
}

/// `continue_kickoff`: start a new kickoff that carries the draft of `session`.
pub(super) fn resume(ctx: &Arc<PlanContext>, conn: &ConnCtx, session: &str) -> Outcome {
    writable(ctx)?;
    let (bridge, state) = (bridge(ctx)?, chat_state(ctx)?);
    let draft = known_draft(ctx, &state, session)?;
    let _starting = state.starting();
    let took = take_authoring(ctx, conn)?;
    let started = continue_draft(ctx, conn, &bridge, &state, &draft);
    if started.is_err() {
        give_back(ctx, conn, &bridge, took);
    }
    started.map(|_| Vec::new())
}

/// Start the continuation, write its draft, then retire the old one. The old file is touched
/// only after the new session started and its draft is in place.
fn continue_draft(
    ctx: &Arc<PlanContext>,
    conn: &ConnCtx,
    bridge: &Bridge,
    state: &Arc<ChatState>,
    draft: &Draft,
) -> Result<String, String> {
    let (inputs, unavailable) = available(ctx, &draft.attachments);
    let small = draft.shape.as_ref().and_then(|shape| shape.size) == Some(KickoffSize::Plan);
    let offer = if small { "kickoff-small" } else { "kickoff" };
    let text = continuation(draft, &unavailable);
    let message = checked(ctx, &text, inputs.clone())?;
    let owner = ChatOwner::default();
    let id = launch(
        ctx,
        conn,
        bridge,
        state,
        Some(offer.into()),
        Some(message),
        owner,
    )?;
    let seed = Seed {
        messages: draft.messages.clone(),
        attachments: inputs,
        shape: draft.shape.clone(),
        name_override: draft.name().map(str::to_owned),
    };
    if let Err(error) = drafts::create(ctx, state, &id, seed) {
        // A running session with no draft is lost at the next restart: end it and refuse.
        if let Err(end) = bridge.end(&id, EndMode::Teardown, EndCause::Caller) {
            tracing::warn!(session = %id, %end, "the continuation could not be ended");
        }
        return Err(error);
    }
    if let Some(shape) = draft.shape.clone() {
        let name = draft.name().map(str::to_owned);
        let view = lock(&state.kickoff).seed(&id, shape, name);
        announce(ctx, view);
    }
    drafts::remove(ctx, state, &draft.session);
    Ok(id)
}

fn continuation(draft: &Draft, unavailable: &[String]) -> String {
    let shape = draft.shape.as_ref();
    let none: &[String] = &[];
    continuation_message(&DraftContinuation {
        name: draft.name(),
        problem: shape.map_or("", |shape| shape.problem.as_str()),
        decisions: shape.map_or(none, |shape| shape.decisions.as_slice()),
        questions: shape.map_or(none, |shape| shape.questions.as_slice()),
        messages: &draft.messages,
        unavailable,
    })
}

/// The inputs that still resolve, and the names of those that do not (a stored file that was
/// swept, a document that was removed): one missing attachment must not refuse the rest.
fn available(ctx: &PlanContext, inputs: &[AttachmentInput]) -> (Vec<AttachmentInput>, Vec<String>) {
    let project = ctx.project();
    let (mut kept, mut missing) = (Vec::new(), Vec::new());
    for input in inputs {
        match resolve_input(&project, &ctx.runtime().attachments, input.clone()) {
            Ok(_) => kept.push(input.clone()),
            Err(_) => missing.push(match input {
                AttachmentInput::File(file) => file.name.clone(),
                AttachmentInput::Document(document) => document.path.clone(),
            }),
        }
    }
    (kept, missing)
}
