//! Setup: shared state, holder-change drops, reconnect replay and open-question broadcasts.

use std::sync::{Arc, Mutex};

use crate::plan_server::authoring::AuthoringEvent;
use crate::plan_server::context::PlanContext;
use crate::plan_server::ids::BrowserId;
use crate::plan_server::wire::message::PlanMessage;
use crate::plan_server::wire::review::ReviewMessage;

use super::questions::open_question_counts;
use super::state::{lock, ReviewState};
use super::suggest::Delivery;

pub(super) fn install(ctx: &Arc<PlanContext>) {
    let state = Arc::new(ReviewState {
        counts: Mutex::new(open_question_counts(&ctx.project())),
        ..ReviewState::default()
    });
    ctx.extensions().insert(Arc::clone(&state));
    follow_authoring(ctx, Arc::clone(&state));
    replay_pending(ctx, Arc::clone(&state));
    follow_counts(ctx, state);
}

fn follow_authoring(ctx: &Arc<PlanContext>, state: Arc<ReviewState>) {
    let weak = Arc::downgrade(ctx);
    ctx.authoring().subscribe(Box::new(move |event| {
        if !matches!(
            event,
            AuthoringEvent::Taken { .. } | AuthoringEvent::Freed { .. }
        ) {
            return;
        }
        let Some(ctx) = weak.upgrade() else {
            return;
        };
        let deliveries = lock(&state.suggestions).drop_for_holder_change();
        // The authoring state changes before subscribers run, so `suggest` can have sent a card
        // to the new holder in between. Read the holder after the drain, which waited for any
        // such `suggest` to release the box.
        let holder = ctx.authoring().holder_browser();
        for delivery in deliveries {
            send_dropped(&ctx, &state, holder.as_ref(), delivery);
        }
    }));
}

/// Tell the sender a suggestion was dropped, and the current holder too so it removes a card it
/// may already hold; a holder that never had the card ignores the status. Each browser gets it
/// once. The frame carries the suggestion id only, never a cookie value.
fn send_dropped(
    ctx: &PlanContext,
    state: &ReviewState,
    holder: Option<&BrowserId>,
    delivery: Delivery,
) {
    let Some(id) = delivery.message["id"].as_str() else {
        return;
    };
    let sender = lock(&state.senders).remove(id);
    let Ok(message) = serde_json::from_value::<PlanMessage>(delivery.message) else {
        return;
    };
    if let Some(sender) = &sender {
        ctx.hub().send_to_browser(sender, &message);
    }
    if let Some(holder) = holder.filter(|holder| sender.as_ref() != Some(*holder)) {
        ctx.hub().send_to_browser(holder, &message);
    }
}

fn replay_pending(ctx: &Arc<PlanContext>, state: Arc<ReviewState>) {
    let weak = Arc::downgrade(ctx);
    ctx.hub().on_open(Box::new(move |conn| {
        let Some(ctx) = weak.upgrade() else {
            return Vec::new();
        };
        if !ctx.authoring().is_holder(&conn.browser) {
            return Vec::new();
        }
        let pending = lock(&state.suggestions).pending_for();
        pending
            .into_iter()
            .filter_map(|message| serde_json::from_value::<PlanMessage>(message).ok())
            .collect()
    }));
}

fn follow_counts(ctx: &Arc<PlanContext>, state: Arc<ReviewState>) {
    ctx.on_batch(Box::new(move |ctx, _batch| {
        let counts = open_question_counts(&ctx.project());
        {
            let mut previous = lock(&state.counts);
            if *previous == counts {
                return;
            }
            *previous = counts.clone();
        }
        ctx.hub()
            .broadcast(&PlanMessage::Review(ReviewMessage::OpenQuestions {
                counts,
            }));
    }));
}
