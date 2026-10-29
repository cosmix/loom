//! Chat delta broadcasts with holder-only queue text and session presence.

use std::sync::Arc;

use super::{team, with_holder, ChatState};
use crate::plan_server::context::PlanContext;
use crate::plan_server::wire::chat::ChatFrame;
use crate::plan_server::wire::message::PlanMessage;
use crate::plan_session::bridge::Bridge;
use crate::plan_session::model::{ChatDelta, QueuedMessage};

pub(super) fn subscribe(ctx: &Arc<PlanContext>, bridge: &Arc<Bridge>, state: &Arc<ChatState>) {
    let (weak, state) = (Arc::downgrade(ctx), Arc::clone(state));
    let events = Arc::downgrade(bridge);
    bridge.subscribe_chat(Box::new(move |change| {
        if let (Some(ctx), Some(bridge)) = (weak.upgrade(), events.upgrade()) {
            delta(&ctx, &state, &bridge, change);
        }
    }));
}

/// Every tab gets every delta; queued texts reach only the holder's tabs.
pub(super) fn delta(ctx: &PlanContext, state: &ChatState, bridge: &Bridge, delta: &ChatDelta) {
    let hub = ctx.hub();
    match delta {
        ChatDelta::Queue { queue } => {
            let hidden = queue.iter().map(|queued| QueuedMessage {
                text: None,
                ..queued.clone()
            });
            let queue = hidden.collect();
            hub.broadcast(&delta_message(ChatDelta::Queue { queue }));
            hub.send_to_holder(state.holder(ctx).as_ref(), &delta_message(delta.clone()));
        }
        ChatDelta::Session { session } => {
            let session = with_holder(ctx, session.clone());
            hub.broadcast(&delta_message(ChatDelta::Session { session }));
        }
        other => hub.broadcast(&delta_message(other.clone())),
    }
    team::observe_kickoff(ctx, state, bridge, delta);
}

pub(super) fn delta_message(delta: ChatDelta) -> PlanMessage {
    PlanMessage::Chat(Box::new(ChatFrame::Delta(delta)))
}
