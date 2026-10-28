//! Browser chat commands: the holder rules and the message checks, then the bridge.
//!
//! Runs on the connection's own thread. Every command is answered with a `result` frame to
//! that connection only; texts an interrupt or an end take off the queue go back to it as
//! `queue_returned` (px-state-23).

use std::sync::Arc;
use std::thread;

use super::guidance::GuidanceSlot;
use super::lease::AuthoringLeases;
use super::ChatState;
use crate::plan_server::authoring::{FreeReason, Refusal, Take};
use crate::plan_server::context::PlanContext;
use crate::plan_server::ids::{ConnCtx, PublicId};
use crate::plan_server::wire::chat::{ChatCommand, ChatFrame, CommandResult};
use crate::plan_server::wire::message::PlanMessage;
use crate::plan_session::attachments::{
    format_message, resolve, Attachment, AttachmentInput, MAX_ATTACHMENTS,
};
use crate::plan_session::bridge::{
    Bridge, BridgeError, EndCause, EndMode, LaunchGuidance, StartRequest,
};
use crate::plan_session::input::{check_delivered, check_send, sanitize, InputRefusal};
use crate::plan_session::kind::{SessionDir, SessionKind};
use crate::plan_session::model::{Origin, SessionState};

const UNAVAILABLE: &str = "the chat is not available";
const NO_CHAT: &str = "no chat is running";
const NO_TURN: &str = "no turn is running";
const CLOSING: &str = "the chat is closing";

/// The queued texts to hand back to the connection, or the error it is shown.
type Outcome = Result<Vec<String>, String>;

/// A checked message: what the session receives and its chips.
struct Message {
    formatted: String,
    attachments: Vec<Attachment>,
}

/// Handle one chat command from `conn`.
pub fn dispatch(ctx: &Arc<PlanContext>, conn: &ConnCtx, cmd: ChatCommand) {
    let client_id = match &cmd {
        ChatCommand::Start { client_id, .. }
        | ChatCommand::Send { client_id, .. }
        | ChatCommand::Withdraw { client_id, .. }
        | ChatCommand::Permission { client_id, .. }
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
            text,
            attachments,
            ..
        } => start(ctx, conn, offer, text, attachments),
        ChatCommand::Send {
            text, attachments, ..
        } => send(ctx, conn, &text, attachments),
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
) -> Outcome {
    refuse_new_work(ctx)?;
    if ctx.read_only().is_read_only() {
        return Err(ctx.refusal_message(&Refusal::ReadOnlyMode));
    }
    let typed = text.is_some() || !inputs.is_empty();
    let text = text.unwrap_or_default();
    let message = typed.then(|| checked(ctx, &text, inputs)).transpose()?;
    let (bridge, state) = (bridge(ctx)?, chat_state(ctx)?);
    let _starting = state.starting();
    let took = take_authoring(ctx, conn)?;
    // The offer is the guidance provider's to accept: plan 7's refuses one the checklist
    // does not offer now, plan 8's also knows its own start ids.
    let started = launch(ctx, conn, &bridge, &state, offer, message);
    if started.is_err() {
        give_back(ctx, conn, &bridge, took);
    }
    started
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
            sent.map(|_| Vec::new()).map_err(|error| error.to_string())
        }
        None => {
            let started = launch(ctx, conn, &bridge, &state, None, Some(message));
            if started.is_err() {
                give_back(ctx, conn, &bridge, took);
            }
            started
        }
    }
}

/// Start an authoring chat; the provider writes its guidance into the session directory.
fn launch(
    ctx: &Arc<PlanContext>,
    conn: &ConnCtx,
    bridge: &Bridge,
    state: &Arc<ChatState>,
    offer: Option<String>,
    message: Option<Message>,
) -> Outcome {
    let Some(slot) = ctx.extensions().get::<GuidanceSlot>() else {
        return Err(UNAVAILABLE.to_owned());
    };
    // px-seams-07: an offer, the user's own first message (no origin), or a New chat.
    let opening_origin = match (&offer, &message) {
        (Some(_), _) => Some(Origin::Offer),
        (None, Some(_)) => None,
        (None, None) => Some(Origin::Start),
    };
    let first_message = message.map(|message| message.formatted);
    let (provider, guide_ctx, guide_offer) = (slot.get(), Arc::clone(ctx), offer.clone());
    let (starter, state) = (conn.public_id.clone(), Arc::clone(state));
    let guidance = Box::new(move |dir: &SessionDir| -> anyhow::Result<LaunchGuidance> {
        let (offer, first) = (guide_offer.as_deref(), first_message.as_deref());
        let guidance = provider.authoring(&guide_ctx, dir, offer, first)?;
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
        extra_env: Vec::new(),
    };
    let started = bridge.start_session(request);
    started
        .map(|_| Vec::new())
        .map_err(|error| error.to_string())
}

/// A holder-only command on the running chat; `not_running` replaces the bridge's
/// `NotRunning` text, which only fits a closing chat.
fn on_chat(
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

fn refuse_new_work(ctx: &PlanContext) -> Result<(), String> {
    match ctx.is_stopping() {
        true => Err(ctx.refusal_message(&Refusal::Stopping)),
        false => Ok(()),
    }
}

/// Take authoring when free; true when this call took it.
fn take_authoring(ctx: &PlanContext, conn: &ConnCtx) -> Result<bool, String> {
    let held = ctx.authoring().is_holder(&conn.browser);
    ctx.require_authoring(&conn.browser, Take::TakeIfFree)
        .map_err(|refusal| ctx.refusal_message(&refusal))?;
    remember_holder(ctx, conn);
    Ok(!held)
}

/// A start that failed frees the authoring it took, unless a chat is reserved or runs by then
/// (another tab of the same browser started one in between, and that chat keeps the hold).
/// The caller holds `ChatState::starting` from its take to here, so no other start's take
/// and reserve lands between this check and the release.
fn give_back(ctx: &PlanContext, conn: &ConnCtx, bridge: &Bridge, took: bool) {
    if took && !chat_open(bridge) {
        let reason = FreeReason::Released("the chat did not start");
        ctx.authoring().release_if(&conn.public_id, reason);
    }
}

/// An ended chat frees its starter's authoring unless a chat is reserved or runs by then: the
/// bridge drops the ended chat before its `Ended` is heard, so a start can reserve another in
/// between. Decided under `ChatState::starting`, only tried here because a start's own events
/// can reach the listener on the start's thread. While a start holds it, a thread of its own
/// waits for that start to open its chat or give authoring back, then decides; keeping the
/// hold instead would leave it held with no chat after a start the holder's browser had not
/// taken authoring for (its `give_back` frees only what it took). A lease keeps the hold at
/// the moment of the decision, so one taken while the thread waited counts too.
pub(super) fn release_ended(
    ctx: &Arc<PlanContext>,
    state: &Arc<ChatState>,
    bridge: &Arc<Bridge>,
    leases: &Arc<AuthoringLeases>,
    starter: PublicId,
) {
    if let Some(_starting) = state.try_starting() {
        release_unless_open(ctx, bridge, leases, &starter);
        return;
    }
    let (ctx, state, bridge) = (Arc::clone(ctx), Arc::clone(state), Arc::clone(bridge));
    let leases = Arc::clone(leases);
    let waiting = thread::Builder::new()
        .name("loom-plan-release".into())
        .spawn(move || {
            let _starting = state.starting();
            release_unless_open(&ctx, &bridge, &leases, &starter);
        });
    if let Err(error) = waiting {
        tracing::warn!("authoring stays held after its chat ended: {error}");
    }
}

fn release_unless_open(
    ctx: &PlanContext,
    bridge: &Bridge,
    leases: &AuthoringLeases,
    starter: &PublicId,
) {
    if !leases.held() && !chat_open(bridge) {
        ctx.authoring()
            .release_if(starter, FreeReason::Released("chat ended"));
    }
}

/// A chat is reserved or runs. The bridge lists a reserved chat before it names it the chat
/// session, so the listing decides: `chat_session_id` stays empty while a start runs its
/// guidance.
fn chat_open(bridge: &Bridge) -> bool {
    bridge.sessions().iter().any(|(_, kind, state)| {
        *kind != SessionKind::Summary
            && !matches!(state, SessionState::Ended | SessionState::Failed)
    })
}

fn remember_holder(ctx: &PlanContext, conn: &ConnCtx) {
    if let Some(state) = ctx.extensions().get::<ChatState>() {
        state.remember_holder(&conn.browser);
    }
}

fn bridge(ctx: &PlanContext) -> Result<Arc<Bridge>, String> {
    ctx.extensions()
        .get::<Bridge>()
        .ok_or_else(|| UNAVAILABLE.to_owned())
}

fn chat_state(ctx: &PlanContext) -> Result<Arc<ChatState>, String> {
    ctx.extensions()
        .get::<ChatState>()
        .ok_or_else(|| UNAVAILABLE.to_owned())
}

/// Resolve the attachments server-side, then the send checks on what the session receives.
fn checked(ctx: &PlanContext, text: &str, inputs: Vec<AttachmentInput>) -> Result<Message, String> {
    if inputs.len() > MAX_ATTACHMENTS {
        return Err(InputRefusal::TooManyAttachments.to_string());
    }
    let resolved: Result<Vec<Attachment>, _> = {
        let project = ctx.project();
        inputs
            .into_iter()
            .map(|input| resolve(&project, input))
            .collect()
    };
    let attachments = resolved.map_err(|error| error.to_string())?;
    // Sanitised first: a control character before a `/` would hide the command from the check.
    let text = sanitize(text);
    check_send(&text).map_err(|refusal| refusal.to_string())?;
    let formatted = sanitize(&format_message(&text, &attachments));
    check_delivered(&formatted).map_err(|refusal| refusal.to_string())?;
    Ok(Message {
        formatted,
        attachments,
    })
}
