//! Client messages arriving on a planning WebSocket.
//!
//! [`dispatch`] runs on the connection's own thread, between reads of its socket: a handler
//! must return quickly and hand long work to a thread of its own, or that tab stops receiving
//! frames. A frame that does not parse as a [`ClientMessage`] never gets here; the router
//! answers it with `Refused { command: "frame", .. }`.

use std::sync::Arc;

use crate::plan_server::context::PlanContext;
use crate::plan_server::ids::ConnCtx;
use crate::plan_server::wire::message::ClientMessage;

/// Handle one client message from `conn`. One arm per [`ClientMessage`] variant.
pub fn dispatch(ctx: &Arc<PlanContext>, conn: &ConnCtx, msg: ClientMessage) {
    match msg {
        ClientMessage::SetName { name } => {
            ctx.presence().set_name(&conn.browser, &name);
            // The holder's name also appears in the authoring view.
            ctx.broadcast_people();
        }
        ClientMessage::Reading { path } => {
            ctx.presence().set_reading(conn.id, path);
            ctx.broadcast_presence();
        }
        ClientMessage::Chat(cmd) => crate::plan_server::chat::dispatch(ctx, conn, cmd),
    }
}
