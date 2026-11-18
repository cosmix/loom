//! The holder's fetch commands from a `WebFetch` card and the chat's grant line. Each goes
//! through `on_chat`, the authorization path of Allow and Deny.

use super::dispatch::{on_chat, Outcome, CLOSING};
use crate::plan_server::context::PlanContext;
use crate::plan_server::ids::ConnCtx;
use crate::plan_server::web_fetch::settings;
use crate::plan_session::bridge::BridgeError;

/// "Allow this session": the card and the session's later fetches are allowed.
pub(super) fn allow_session(ctx: &PlanContext, conn: &ConnCtx, request_id: &str) -> Outcome {
    on_chat(ctx, conn, CLOSING, |bridge, id| {
        bridge.grant_fetches(id, request_id).map(|()| Vec::new())
    })
}

/// `"Always allow <host>"`: the host the server reads from the card's URL goes on the allowlist,
/// then every open fetch card the rules now cover is answered, this one included.
pub(super) fn allow_host(ctx: &PlanContext, conn: &ConnCtx, request_id: &str) -> Outcome {
    let settings = settings(ctx);
    on_chat(ctx, conn, CLOSING, |bridge, id| {
        // A card answered between this read and `answer_fetches` is still answered by the
        // rules and still lists the host: the holder chose "Always allow" for it.
        let host = bridge.fetch_host(id, request_id)?;
        settings.allow_host(&host).map_err(BridgeError::Refused)?;
        bridge.answer_fetches(id).map(|()| Vec::new())
    })
}

/// "Ask again": the session's grant ends.
pub(super) fn ask_again(ctx: &PlanContext, conn: &ConnCtx) -> Outcome {
    on_chat(ctx, conn, CLOSING, |bridge, id| {
        bridge.revoke_fetches(id).map(|()| Vec::new())
    })
}
