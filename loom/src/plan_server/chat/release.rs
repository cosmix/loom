//! Giving authoring back: after a start that failed and after a chat that ended.

use std::sync::Arc;
use std::thread;

use super::lease::AuthoringLeases;
use super::ChatState;
use crate::plan_server::authoring::FreeReason;
use crate::plan_server::context::PlanContext;
use crate::plan_server::ids::{ConnCtx, PublicId};
use crate::plan_session::bridge::Bridge;
use crate::plan_session::kind::SessionKind;
use crate::plan_session::model::SessionState;

/// A start that failed frees the authoring it took, unless a chat is reserved or runs by then
/// (another tab of the same browser started one in between, and that chat keeps the hold).
/// The caller holds `ChatState::starting` from its take to here, so no other start's take
/// and reserve lands between this check and the release.
pub(super) fn give_back(ctx: &PlanContext, conn: &ConnCtx, bridge: &Bridge, took: bool) {
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
    let (worker_ctx, state, bridge) = (Arc::clone(ctx), Arc::clone(state), Arc::clone(bridge));
    let leases = Arc::clone(leases);
    let waiting = thread::Builder::new()
        .name("loom-plan-release".into())
        .spawn(move || {
            let _starting = state.starting();
            release_unless_open(&worker_ctx, &bridge, &leases, &starter);
        });
    match waiting {
        Ok(worker) => ctx.own_worker("authoring release", worker, None),
        Err(error) => tracing::warn!("authoring stays held after its chat ended: {error}"),
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
pub(super) fn chat_open(bridge: &Bridge) -> bool {
    bridge.sessions().iter().any(|(_, kind, state)| {
        *kind != SessionKind::Summary
            && !matches!(state, SessionState::Ended | SessionState::Failed)
    })
}
