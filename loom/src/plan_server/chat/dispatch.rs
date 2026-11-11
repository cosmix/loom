//! Browser chat commands: the holder rules and the message checks, then the bridge.
//!
//! Runs on the connection's own thread. Every command is answered with a `result` frame to
//! that connection only; texts an interrupt or an end take off the queue go back to it as
//! `queue_returned` (px-state-23).

use std::sync::{Arc, PoisonError};

use super::guidance::{validate_feature, validate_plan, GuidanceSlot};
use super::release::give_back;
use super::{draft_cmd, fetch_cmd, kickoff_cmd, ChatState};
use crate::plan_server::authoring::{Refusal, Take};
use crate::plan_server::context::PlanContext;
use crate::plan_server::ids::ConnCtx;
use crate::plan_server::wire::chat::{ChatCommand, ChatFrame, CommandResult};
use crate::plan_server::wire::message::PlanMessage;
use crate::plan_session::attachments::{
    format_message, resolve_input, without_file_paths, Attachment, AttachmentInput, MAX_ATTACHMENTS,
};
use crate::plan_session::bridge::{
    Bridge, BridgeError, EndCause, EndMode, LaunchGuidance, StartRequest,
};
use crate::plan_session::commands::CommandTable;
use crate::plan_session::input::{
    check_alone, check_delivered, check_send, sanitize, InputRefusal,
};
use crate::plan_session::kind::{SessionDir, SessionKind};
use crate::plan_session::model::Origin;

const UNAVAILABLE: &str = "the chat is not available";
pub(super) const NO_CHAT: &str = "no chat is running";
const NO_TURN: &str = "no turn is running";
const NO_MESSAGE: &str = "a chat starts with your first message";
pub(super) const CLOSING: &str = "the chat is closing";

/// The queued texts to hand back to the connection, or the error it is shown.
pub(super) type Outcome = Result<Vec<String>, String>;

/// What a new chat belongs to: a feature, a standalone plan's slug, or the whole project.
#[derive(Default)]
pub(super) struct ChatOwner {
    feature: Option<String>,
    plan: Option<String>,
}

/// A checked message: what the session receives and its chips.
pub(super) struct Message {
    pub(super) formatted: String,
    pub(super) attachments: Vec<Attachment>,
}

/// Handle one chat command from `conn`.
pub fn dispatch(ctx: &Arc<PlanContext>, conn: &ConnCtx, cmd: ChatCommand) {
    let client_id = match &cmd {
        ChatCommand::Start { client_id, .. }
        | ChatCommand::Send { client_id, .. }
        | ChatCommand::ConfirmKickoff { client_id, .. }
        | ChatCommand::SetAsideKickoff { client_id, .. }
        | ChatCommand::ContinueKickoff { client_id, .. }
        | ChatCommand::DiscardKickoff { client_id, .. }
        | ChatCommand::Withdraw { client_id, .. }
        | ChatCommand::Permission { client_id, .. }
        | ChatCommand::AllowSessionFetches { client_id, .. }
        | ChatCommand::AllowFetchHost { client_id, .. }
        | ChatCommand::AskFetchesAgain { client_id }
        | ChatCommand::Interrupt { client_id }
        | ChatCommand::End { client_id } => client_id.clone(),
    };
    let outcome = run(ctx, conn, cmd);
    answer(ctx, conn, client_id, outcome);
}

fn run(ctx: &Arc<PlanContext>, conn: &ConnCtx, cmd: ChatCommand) -> Outcome {
    match cmd {
        ChatCommand::Start {
            offer,
            feature,
            plan,
            text,
            attachments,
            ..
        } => {
            let owner = ChatOwner { feature, plan };
            start(ctx, conn, offer, text, attachments, owner)
        }
        ChatCommand::Send {
            text, attachments, ..
        } => send(ctx, conn, &text, attachments),
        ChatCommand::ConfirmKickoff { size, name, .. } => {
            kickoff_cmd::confirm(ctx, conn, size, &name)
        }
        ChatCommand::SetAsideKickoff { session, .. } => draft_cmd::set_aside(ctx, conn, &session),
        ChatCommand::ContinueKickoff { session, .. } => draft_cmd::resume(ctx, conn, &session),
        ChatCommand::DiscardKickoff { session, .. } => draft_cmd::discard(ctx, conn, &session),
        ChatCommand::Withdraw { queue_id, .. } => on_chat(ctx, conn, CLOSING, |bridge, id| {
            bridge.withdraw(id, &queue_id).map(|()| Vec::new())
        }),
        ChatCommand::Permission {
            request_id,
            allow,
            note,
            ..
        } => on_chat(ctx, conn, CLOSING, |bridge, id| {
            let note = note.map(|note| sanitize(&note));
            bridge.answer_permission(id, &request_id, allow, note)?;
            Ok(Vec::new())
        }),
        ChatCommand::AllowSessionFetches { request_id, .. } => {
            fetch_cmd::allow_session(ctx, conn, &request_id)
        }
        ChatCommand::AllowFetchHost { request_id, .. } => {
            fetch_cmd::allow_host(ctx, conn, &request_id)
        }
        ChatCommand::AskFetchesAgain { .. } => fetch_cmd::ask_again(ctx, conn),
        ChatCommand::Interrupt { .. } => {
            on_chat(ctx, conn, NO_TURN, |bridge, id| bridge.interrupt(id))
        }
        ChatCommand::End { .. } => on_chat(ctx, conn, CLOSING, |bridge, id| {
            bridge.end(id, EndMode::FinalWrite, EndCause::Holder)
        }),
    }
}

fn answer(ctx: &PlanContext, conn: &ConnCtx, client_id: Option<String>, outcome: Outcome) {
    let (returned, error) = match outcome {
        Ok(texts) => (texts, None),
        Err(error) => (Vec::new(), Some(error)),
    };
    let result = CommandResult {
        client_id,
        ok: error.is_none(),
        error,
    };
    let hub = ctx.hub();
    hub.send_to_conn(
        conn.id,
        &PlanMessage::Chat(Box::new(ChatFrame::Result(result))),
    );
    if !returned.is_empty() {
        let frame = ChatFrame::QueueReturned { texts: returned };
        hub.send_to_conn(conn.id, &PlanMessage::Chat(Box::new(frame)));
    }
}

fn start(
    ctx: &Arc<PlanContext>,
    conn: &ConnCtx,
    offer: Option<String>,
    text: Option<String>,
    inputs: Vec<AttachmentInput>,
    owner: ChatOwner,
) -> Outcome {
    let typed = text.as_deref().is_some_and(|text| !text.trim().is_empty()) || !inputs.is_empty();
    if offer.is_none() && !typed {
        return Err(NO_MESSAGE.to_owned());
    }
    draft_cmd::writable(ctx)?;
    let text = text.unwrap_or_default();
    let draft = draft_cmd::StartDraft::of(offer.as_deref(), &text, &inputs);
    // No session exists yet: its command list is still loading (B-D2).
    let loading = CommandTable::loading();
    let message = typed
        .then(|| checked_against(ctx, &text, inputs, &loading))
        .transpose()?;
    let (bridge, state) = (bridge(ctx)?, chat_state(ctx)?);
    let _starting = state.starting();
    let took = take_authoring(ctx, conn)?;
    // The offer is the guidance provider's to accept: plan 7's refuses one the checklist
    // does not offer now, plan 8's also knows its own start ids.
    let started = launch(ctx, conn, &bridge, &state, offer, message, owner);
    match &started {
        Ok(id) => draft.record(ctx, &state, id),
        Err(_) => give_back(ctx, conn, &bridge, took),
    }
    started.map(|_| Vec::new())
}

/// A send to the running chat, or the first message of a new one.
fn send(
    ctx: &Arc<PlanContext>,
    conn: &ConnCtx,
    text: &str,
    inputs: Vec<AttachmentInput>,
) -> Outcome {
    refuse_new_work(ctx)?;
    let message = checked(ctx, text, inputs)?;
    let (bridge, state) = (bridge(ctx)?, chat_state(ctx)?);
    // Only a send that may start the chat waits behind a start; under the lock the second
    // look sees the chat that start opened. A chat that ends after the first look refuses
    // the send, as one ending during the send does.
    let running = bridge.chat_session_id();
    let _starting = running.is_none().then(|| state.starting());
    let took = take_authoring(ctx, conn)?;
    match running.or_else(|| bridge.chat_session_id()) {
        Some(id) => {
            let sent = bridge.send(&id, message.formatted, message.attachments);
            if sent.is_ok() {
                draft_cmd::on_send(ctx, &state, &id, text);
            }
            sent.map(|_| Vec::new()).map_err(|error| error.to_string())
        }
        None => {
            let owner = retained_owner(ctx, &bridge);
            let started = launch(ctx, conn, &bridge, &state, None, Some(message), owner);
            if started.is_err() {
                give_back(ctx, conn, &bridge, took);
            }
            started.map(|_| Vec::new())
        }
    }
}

/// A send's fallback start replaces a chat that ended just before it, so it stays where that chat
/// was. A kickoff can name a feature it has not written yet: such an owner is dropped, since
/// refusing the message over it would lose what the user typed.
fn retained_owner(ctx: &PlanContext, bridge: &Bridge) -> ChatOwner {
    let (feature, plan) = bridge.last_chat_owner();
    let project = ctx.project();
    ChatOwner {
        feature: feature.filter(|name| validate_feature(&project, Some(name)).is_ok()),
        plan: plan.filter(|name| validate_plan(&project, None, Some(name)).is_ok()),
    }
}

/// Start an authoring chat, the provider writing its guidance into the session directory;
/// the new session's id.
pub(super) fn launch(
    ctx: &Arc<PlanContext>,
    conn: &ConnCtx,
    bridge: &Bridge,
    state: &Arc<ChatState>,
    offer: Option<String>,
    message: Option<Message>,
    ChatOwner { feature, plan }: ChatOwner,
) -> Result<String, String> {
    let Some(slot) = ctx.extensions().get::<GuidanceSlot>() else {
        return Err(UNAVAILABLE.to_owned());
    };
    // An offer, or the user's own first message (no origin); `start` refuses neither.
    let opening_origin = offer.is_some().then_some(Origin::Offer);
    let extra_env = super::team::extra_env(offer.as_deref(), feature.as_deref())?;
    let first_message = message.map(|message| message.formatted);
    let (provider, guide_ctx, guide_offer) = (slot.get(), Arc::clone(ctx), offer.clone());
    let (starter, state) = (conn.public_id.clone(), Arc::clone(state));
    let (session_feature, session_plan) = (feature.clone(), plan.clone());
    let guidance = Box::new(move |dir: &SessionDir| -> anyhow::Result<LaunchGuidance> {
        let (offer, first) = (guide_offer.as_deref(), first_message.as_deref());
        validate_plan(&guide_ctx.project(), feature.as_deref(), plan.as_deref())?;
        let guidance = provider.authoring(&guide_ctx, dir, offer, first, feature.as_deref())?;
        super::team::prepare_scope(&guide_ctx, dir, offer, feature.as_deref())?;
        // Recorded before the bridge can launch (and fail) the session, so its `Ended`
        // always finds the starter; the directory is named after the session id.
        if let Some(id) = dir.path().file_name() {
            state.started(id.to_string_lossy().into_owned(), starter);
        }
        Ok(guidance)
    });
    let request = StartRequest {
        kind: SessionKind::Authoring,
        guidance,
        opening_origin,
        offer,
        feature: session_feature,
        plan: session_plan,
        extra_env,
    };
    bridge
        .start_session(request)
        .map_err(|error| error.to_string())
}

/// A holder-only command on the running chat; `not_running` replaces the bridge's
/// `NotRunning` text, which only fits a closing chat.
pub(super) fn on_chat(
    ctx: &PlanContext,
    conn: &ConnCtx,
    not_running: &str,
    command: impl FnOnce(&Bridge, &str) -> Result<Vec<String>, BridgeError>,
) -> Outcome {
    ctx.require_authoring(&conn.browser, Take::HolderOnly)
        .map_err(|refusal| ctx.refusal_message(&refusal))?;
    remember_holder(ctx, conn);
    let bridge = bridge(ctx)?;
    let id = bridge.chat_session_id().ok_or_else(|| NO_CHAT.to_owned())?;
    command(&bridge, &id).map_err(|error| match error {
        BridgeError::NotRunning => not_running.to_owned(),
        other => other.to_string(),
    })
}

pub(super) fn refuse_new_work(ctx: &PlanContext) -> Result<(), String> {
    match ctx.is_stopping() {
        true => Err(ctx.refusal_message(&Refusal::Stopping)),
        false => Ok(()),
    }
}

/// Take authoring when free; true when this call took it.
pub(super) fn take_authoring(ctx: &PlanContext, conn: &ConnCtx) -> Result<bool, String> {
    let held = ctx.authoring().is_holder(&conn.browser);
    ctx.require_authoring(&conn.browser, Take::TakeIfFree)
        .map_err(|refusal| ctx.refusal_message(&refusal))?;
    remember_holder(ctx, conn);
    Ok(!held)
}

pub(super) fn remember_holder(ctx: &PlanContext, conn: &ConnCtx) {
    if let Some(state) = ctx.extensions().get::<ChatState>() {
        state.remember_holder(&conn.browser);
    }
}

pub(super) fn bridge(ctx: &PlanContext) -> Result<Arc<Bridge>, String> {
    ctx.extensions()
        .get::<Bridge>()
        .ok_or_else(|| UNAVAILABLE.to_owned())
}

pub(super) fn chat_state(ctx: &PlanContext) -> Result<Arc<ChatState>, String> {
    ctx.extensions()
        .get::<ChatState>()
        .ok_or_else(|| UNAVAILABLE.to_owned())
}

/// The chat session's command table now; a send with no chat session yet starts one, whose
/// list is still loading.
fn chat_table(ctx: &PlanContext) -> Arc<CommandTable> {
    let bridge = ctx.extensions().get::<Bridge>();
    let cell = bridge.and_then(|bridge| bridge.command_cell(&bridge.chat_session_id()?));
    let table = cell.map(|cell| Arc::clone(&cell.read().unwrap_or_else(PoisonError::into_inner)));
    table.unwrap_or_else(|| Arc::new(CommandTable::loading()))
}

/// [`checked_against`] the chat session's command table now.
pub(super) fn checked(
    ctx: &PlanContext,
    text: &str,
    inputs: Vec<AttachmentInput>,
) -> Result<Message, String> {
    checked_against(ctx, text, inputs, &chat_table(ctx))
}

/// Resolve the attachments server-side, then the send checks on what the session receives,
/// a slash message against `table`.
fn checked_against(
    ctx: &PlanContext,
    text: &str,
    inputs: Vec<AttachmentInput>,
    table: &CommandTable,
) -> Result<Message, String> {
    if inputs.len() > MAX_ATTACHMENTS {
        return Err(InputRefusal::TooManyAttachments.to_string());
    }
    // Sanitised first: a control character before a `/` would hide the command from the checks.
    let text = sanitize(text);
    check_alone(&text, inputs.len()).map_err(|refusal| refusal.to_string())?;
    let resolved: Result<Vec<Attachment>, _> = {
        let (project, files) = (ctx.project(), &ctx.runtime().attachments);
        inputs
            .into_iter()
            .map(|input| resolve_input(&project, files, input))
            .collect()
    };
    let attachments = resolved.map_err(|error| error.to_string())?;
    check_send(&text, table).map_err(|refusal| refusal.to_string())?;
    let formatted = sanitize(&format_message(&text, &attachments));
    check_delivered(&formatted).map_err(|refusal| refusal.to_string())?;
    // The block names a file's absolute path; the chips browsers get never do.
    let attachments = without_file_paths(attachments);
    Ok(Message {
        formatted,
        attachments,
    })
}
