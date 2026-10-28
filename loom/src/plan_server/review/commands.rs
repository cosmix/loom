//! WebSocket review commands: suggest and suggestion_answer.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::plan_server::authoring::Take;
use crate::plan_server::context::PlanContext;
use crate::plan_server::ids::{ConnCtx, PublicId};
use crate::plan_server::wire::message::PlanMessage;
use crate::plan_server::wire::review::{ReviewCommand, ReviewMessage, SuggestionStatus};

use super::state::ReviewState;
use super::suggest::{BrowserRef, SuggestRefusal, SuggestedPassage, SuggestionAction};

pub(crate) fn dispatch(ctx: &Arc<PlanContext>, conn: &ConnCtx, cmd: ReviewCommand) {
    let result = match cmd {
        ReviewCommand::Suggest { passage, note } => suggest(ctx, conn, passage, &note),
        ReviewCommand::SuggestionAnswer { id, action } => answer(ctx, conn, &id, action),
    };
    if let Err(reason) = result {
        ctx.hub().send_to_conn(
            conn.id,
            &PlanMessage::Review(ReviewMessage::Refused { reason }),
        );
    }
}

fn suggest(
    ctx: &PlanContext,
    conn: &ConnCtx,
    passage: SuggestedPassage,
    note: &str,
) -> Result<(), String> {
    if ctx.authoring().is_holder(&conn.browser) {
        return Err(refusal_text(&SuggestRefusal::SenderIsHolder));
    }
    let state = review_state(ctx)?;
    let mut suggestions = lock(&state.suggestions);
    let (holder, away) = holder_ref(ctx)?;
    let sender = browser_ref(&conn.public_id, ctx.presence().name_of(&conn.browser))?;
    // SuggestedPassage is the wire type's re-export; all passage fields are preserved.
    let delivery = suggestions
        .submit(&sender, Some(&holder), passage, note)
        .map_err(|error| refusal_text(&error))?;
    let message: PlanMessage = serde_json::from_value(delivery.message)
        .map_err(|_| "Could not read the suggestion delivery".to_owned())?;
    let PlanMessage::Review(ReviewMessage::Suggestion { id, .. }) = &message else {
        return Err("Could not read the suggestion delivery".to_owned());
    };
    // Keep the box locked until routing is recorded, so a holder-change drop can find it.
    lock(&state.senders).insert(id.clone(), conn.browser.clone());
    drop(suggestions);
    // The cookie id only routes the frame; the message carries public ids alone. An away
    // holder has no tab open and gets the pending card from setup's on_open replay.
    let holder_browser = ctx.authoring().holder_browser();
    ctx.hub().send_to_holder(holder_browser.as_ref(), &message);
    ctx.hub().send_to_browser(
        &conn.browser,
        &PlanMessage::Review(ReviewMessage::SuggestionStatus {
            id: id.clone(),
            status: SuggestionStatus::Sent {
                holder: holder.name,
                away,
            },
        }),
    );
    Ok(())
}

fn answer(
    ctx: &PlanContext,
    conn: &ConnCtx,
    id: &str,
    action: SuggestionAction,
) -> Result<(), String> {
    ctx.require_authoring(&conn.browser, Take::HolderOnly)
        .map_err(|refusal| match refusal {
            crate::plan_server::authoring::Refusal::NotHolder { .. } => {
                refusal_text(&SuggestRefusal::NotHolder)
            }
            _ => ctx.refusal_message(&refusal),
        })?;
    let state = review_state(ctx)?;
    let mut suggestions = lock(&state.suggestions);
    let (holder, _) = holder_ref(ctx)?;
    let by = browser_ref(&conn.public_id, ctx.presence().name_of(&conn.browser))?;
    let delivery = suggestions
        .answer(&by, Some(&holder), id, action)
        .map_err(|error| refusal_text(&error))?;
    let sender = lock(&state.senders).remove(id);
    drop(suggestions);
    let message = serde_json::from_value::<PlanMessage>(delivery.message)
        .map_err(|_| "Could not read the suggestion status".to_owned())?;
    if let Some(sender) = sender {
        ctx.hub().send_to_browser(&sender, &message);
    }
    Ok(())
}

fn review_state(ctx: &PlanContext) -> Result<Arc<ReviewState>, String> {
    ctx.extensions()
        .get::<ReviewState>()
        .ok_or_else(|| "Review suggestions are unavailable".to_owned())
}

fn holder_ref(ctx: &PlanContext) -> Result<(BrowserRef, bool), String> {
    let holder = ctx
        .authoring()
        .view(ctx.presence())
        .holder
        .ok_or_else(|| refusal_text(&SuggestRefusal::NoHolder))?;
    let away = holder.away_since.is_some();
    Ok((browser_ref(&holder.public_id, holder.name)?, away))
}

fn browser_ref(public_id: &PublicId, name: String) -> Result<BrowserRef, String> {
    // PublicId is a transparent string newtype with no string accessor.
    let id = serde_json::to_value(public_id)
        .and_then(serde_json::from_value::<String>)
        .map_err(|_| "Could not read the public browser id".to_owned())?;
    Ok(BrowserRef {
        public_id: id.clone(),
        id,
        name,
    })
}

fn refusal_text(refusal: &SuggestRefusal) -> String {
    match refusal {
        SuggestRefusal::NoHolder => "Nobody holds authoring".to_owned(),
        SuggestRefusal::SenderIsHolder => "This browser holds authoring".to_owned(),
        SuggestRefusal::NotHolder => "Only the author can answer a suggestion".to_owned(),
        SuggestRefusal::NoteTooLong => "The note is longer than 1,000 characters".to_owned(),
        SuggestRefusal::TooMany => "20 suggestions are already waiting".to_owned(),
        SuggestRefusal::BadPath(reason) => format!("Invalid suggestion path: {reason}"),
        SuggestRefusal::UnknownId => "Unknown suggestion".to_owned(),
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
