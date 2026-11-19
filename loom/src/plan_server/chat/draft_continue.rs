//! `continue_kickoff`: a new kickoff that carries an unfinished draft, resuming the confirmed
//! document when the draft already wrote one.

use std::path::Path;
use std::sync::Arc;

use super::dispatch::{bridge, chat_state, launch, take_authoring, ChatOwner, Outcome};
use super::draft_cmd::{known_draft, on_confirm, writable};
use super::drafts::{self, Draft, Seed};
use super::kickoff_cmd::record_owner;
use super::lock;
use super::message::checked;
use super::release::give_back;
use super::ChatState;
use crate::plan_server::context::PlanContext;
use crate::plan_server::ids::ConnCtx;
use crate::plan_server::wire::kickoff::KickoffView;
use crate::plan_server::wire::message::PlanMessage;
use crate::plan_session::attachments::{format_message, resolve_input, AttachmentInput};
use crate::plan_session::bridge::{Bridge, EndCause, EndMode};
use crate::plan_session::scope::{KickoffGrant, KickoffSize};
use crate::planning::guidance::{continuation_message, DraftContinuation};

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
    let (inputs, unavailable, reserved) = available(ctx, &draft.attachments);
    let offer = match draft.size {
        KickoffSize::Feature => "kickoff",
        KickoffSize::Plan => "kickoff-small",
    };
    let resumed = resumed(ctx.root(), draft);
    let grant = resumed.as_ref().map(|resumed| resumed.grant.clone());
    let target = grant.as_ref().and(draft.target.as_deref());
    // The opening says the document exists only when it does: a bundle alone resumes the grant
    // (so the bundle can be written) without that claim.
    let said = target.filter(|_| resumed.as_ref().is_some_and(|resumed| resumed.document));
    let text = continuation(draft, said, &unavailable, reserved);
    let message = checked(ctx, &text, inputs.clone())?;
    let mut owner = ChatOwner::default();
    owner.resume.clone_from(&grant);
    let offer = Some(offer.into());
    let id = launch(ctx, conn, bridge, state, offer, Some(message), owner)?;
    let seed = Seed {
        messages: draft.messages.clone(),
        attachments: inputs,
        shape: draft.shape.clone(),
        size: draft.size,
    };
    if let Err(error) = drafts::create(ctx, state, &id, seed) {
        // A running session with no draft is lost at the next restart: end it and refuse.
        if let Err(end) = bridge.end(&id, EndMode::Teardown, EndCause::Caller) {
            tracing::warn!(session = %id, %end, "the continuation could not be ended");
        }
        return Err(error);
    }
    if let (Some(grant), Some(target)) = (&grant, target) {
        record_owner(bridge, &id, grant.size, &grant.name);
        on_confirm(ctx, state, &id, target);
    }
    announce_seeded(ctx, state, &id, draft, target);
    drafts::remove(ctx, state, &draft.session);
    Ok(id)
}

/// What a continuation takes over from a draft that wrote something.
struct Resumed {
    grant: KickoffGrant,
    /// Whether the confirmed document itself exists, not only its bundle.
    document: bool,
}

/// The grant the continuation holds from its first turn: only for a draft whose confirmed
/// document (or its bundle) exists, so the agent can finish it. A draft with nothing written
/// continues as a fresh kickoff of the same size.
fn resumed(root: &Path, draft: &Draft) -> Option<Resumed> {
    let target = draft.target.as_deref()?;
    let written = drafts::written(root, target).ok()?;
    if written.paths.is_empty() {
        return None;
    }
    let document = written.paths.iter().any(|path| path == target);
    Some(Resumed {
        grant: written.grant,
        document,
    })
}

/// Seed the tracker with the draft's shape so Taking shape starts filled, and tell the browsers.
/// A resumed draft's view also carries its target, so the Write button reads written.
fn announce_seeded(
    ctx: &PlanContext,
    state: &ChatState,
    id: &str,
    draft: &Draft,
    target: Option<&str>,
) {
    let Some(shape) = draft.shape.clone() else {
        return;
    };
    let view = {
        let mut tracker = lock(&state.kickoff);
        let seeded = tracker.seed(id, draft.size, shape);
        target
            .and_then(|target| tracker.confirm(id, target))
            .unwrap_or(seeded)
    };
    announce(ctx, view);
}

fn announce(ctx: &PlanContext, view: KickoffView) {
    ctx.hub().broadcast(&PlanMessage::Kickoff(Box::new(view)));
}

fn continuation(
    draft: &Draft,
    target: Option<&str>,
    unavailable: &[String],
    reserved: usize,
) -> String {
    let shape = draft.shape.as_ref();
    let none: &[String] = &[];
    continuation_message(&DraftContinuation {
        name: draft.name(),
        target,
        problem: shape.map_or("", |shape| shape.problem.as_str()),
        decisions: shape.map_or(none, |shape| shape.decisions.as_slice()),
        questions: shape.map_or(none, |shape| shape.questions.as_slice()),
        messages: &draft.messages,
        unavailable,
        reserved,
    })
}

/// The inputs that still resolve, the names of those that do not (a stored file that was
/// swept, a document that was removed: one missing attachment must not refuse the rest), and the
/// bytes the attachment block of the resolved ones adds to a message.
fn available(
    ctx: &PlanContext,
    inputs: &[AttachmentInput],
) -> (Vec<AttachmentInput>, Vec<String>, usize) {
    let project = ctx.project();
    let (mut kept, mut missing, mut resolved) = (Vec::new(), Vec::new(), Vec::new());
    for input in inputs {
        match resolve_input(&project, &ctx.runtime().attachments, input.clone()) {
            Ok(attachment) => {
                kept.push(input.clone());
                resolved.push(attachment);
            }
            Err(_) => missing.push(match input {
                AttachmentInput::File(file) => file.name.clone(),
                AttachmentInput::Document(document) => document.path.clone(),
            }),
        }
    }
    (kept, missing, format_message("", &resolved).len())
}
