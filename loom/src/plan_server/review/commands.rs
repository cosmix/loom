//! WebSocket review commands: suggest and suggestion_answer.

use std::sync::Arc;

use crate::plan_server::authoring::{Refusal, Take};
use crate::plan_server::context::PlanContext;
use crate::plan_server::files::doc::discovered;
use crate::plan_server::ids::{BrowserId, ConnCtx, PublicId};
use crate::plan_server::wire::message::PlanMessage;
use crate::plan_server::wire::review::{ReviewCommand, ReviewMessage, SuggestionStatus};

use super::state::{lock, ReviewState};
use super::suggest::{
    validate_path, BrowserRef, SuggestRefusal, SuggestedPassage, SuggestionAction,
};

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
    require_document(ctx, &passage.path)?;
    let state = review_state(ctx)?;
    let mut suggestions = lock(&state.suggestions);
    let holder = holder_snapshot(ctx)?;
    let sender = browser_ref(&conn.public_id, ctx.presence().name_of(&conn.browser))?;
    // SuggestedPassage is the wire type's re-export; all passage fields are preserved.
    let delivery = suggestions
        .submit(&sender, Some(&holder.reference), passage, note)
        .map_err(|error| refusal_text(&error))?;
    let message: PlanMessage = serde_json::from_value(delivery.message)
        .map_err(|_| "Could not read the suggestion delivery".to_owned())?;
    let PlanMessage::Review(ReviewMessage::Suggestion { id, .. }) = &message else {
        return Err("Could not read the suggestion delivery".to_owned());
    };
    lock(&state.senders).insert(id.clone(), conn.browser.clone());
    let sent = PlanMessage::Review(ReviewMessage::SuggestionStatus {
        id: id.clone(),
        status: SuggestionStatus::Sent {
            holder: holder.reference.name,
            away: holder.away,
        },
    });
    // Both frames go out with the box still locked: a holder change waits on the box, so the
    // card reaches only the holder "sent" names and its "dropped" follows "sent". The cookie id
    // only routes the frame; the message carries public ids alone. An away holder has no tab
    // open and gets the pending card from setup's on_open replay.
    ctx.hub().send_to_browser(&holder.browser, &message);
    ctx.hub().send_to_browser(&conn.browser, &sent);
    drop(suggestions);
    Ok(())
}

/// Section 12 "Suggestions and restores": the passage path passes the request-path checks, so
/// a path discovery does not list is refused like a malformed one.
fn require_document(ctx: &PlanContext, path: &str) -> Result<(), String> {
    let rel = validate_path(path).map_err(|error| refusal_text(&error))?;
    discovered(ctx, &rel).map_err(|_| {
        let reason = format!("{} is not a discovered document", rel.display());
        refusal_text(&SuggestRefusal::BadPath(reason))
    })
}

fn answer(
    ctx: &PlanContext,
    conn: &ConnCtx,
    id: &str,
    action: SuggestionAction,
) -> Result<(), String> {
    ctx.require_authoring(&conn.browser, Take::HolderOnly)
        .map_err(|refusal| match refusal {
            Refusal::NotHolder { .. } => refusal_text(&SuggestRefusal::NotHolder),
            _ => ctx.refusal_message(&refusal),
        })?;
    let state = review_state(ctx)?;
    let mut suggestions = lock(&state.suggestions);
    let holder = holder_snapshot(ctx)?;
    let by = browser_ref(&conn.public_id, ctx.presence().name_of(&conn.browser))?;
    let delivery = suggestions
        .answer(&by, Some(&holder.reference), id, action)
        .map_err(|error| refusal_text(&error))?;
    let sender = lock(&state.senders).remove(id);
    drop(suggestions);
    let message = serde_json::from_value::<PlanMessage>(delivery.message)
        .map_err(|_| "Could not read the suggestion status".to_owned())?;
    if let Some(sender) = sender.filter(|sender| *sender != conn.browser) {
        ctx.hub().send_to_browser(&sender, &message);
    }
    // The holder's tabs remove the card only on this status, sibling tabs included.
    ctx.hub().send_to_browser(&conn.browser, &message);
    Ok(())
}

fn review_state(ctx: &PlanContext) -> Result<Arc<ReviewState>, String> {
    ctx.extensions()
        .get::<ReviewState>()
        .ok_or_else(|| "Review suggestions are unavailable".to_owned())
}

/// The authoring holder, read once: the cookie that routes its frames, its public reference
/// and whether it is away.
struct HolderSnapshot {
    browser: BrowserId,
    reference: BrowserRef,
    away: bool,
}

fn holder_snapshot(ctx: &PlanContext) -> Result<HolderSnapshot, String> {
    // The public id and the name derive from the one cookie read here, so the browser a frame
    // is routed to and the holder a status names cannot disagree.
    let browser = ctx
        .authoring()
        .holder_browser()
        .ok_or_else(|| refusal_text(&SuggestRefusal::NoHolder))?;
    let public_id = ctx.presence().public_id(&browser);
    // A holder change between the two reads leaves `away` false; that change's drop follows.
    let away = ctx
        .authoring()
        .holder()
        .is_some_and(|(holder, away_since)| holder == public_id && away_since.is_some());
    let reference = browser_ref(&public_id, ctx.presence().name_of(&browser))?;
    Ok(HolderSnapshot {
        browser,
        reference,
        away,
    })
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
