//! The two read-only routes of the draft store.

use std::sync::Arc;

use super::{get, list, views, ChatState, DETAIL_PREFIX};
use crate::plan_server::request::{ApiRequest, ApiResponse};
use crate::plan_server::wire::kickoff::{KickoffDraftDetail, KickoffDraftList};

fn chat_state(req: &ApiRequest<'_>) -> Result<Arc<ChatState>, ApiResponse> {
    let unavailable = || ApiResponse::error(503, "unavailable", "the chat is not available");
    req.ctx
        .extensions()
        .get::<ChatState>()
        .ok_or_else(unavailable)
}

/// `GET /api/kickoffs`: the unfinished kickoffs, newest first.
pub(super) fn listing(req: &ApiRequest<'_>) -> ApiResponse {
    match chat_state(req) {
        Ok(state) => ApiResponse::json(
            200,
            &KickoffDraftList {
                drafts: list(req.ctx, &state),
            },
        ),
        Err(refusal) => refusal,
    }
}

/// `GET /api/kickoffs/<session>`: one draft with its shape and the user's messages.
pub(super) fn detail(req: &ApiRequest<'_>) -> ApiResponse {
    let state = match chat_state(req) {
        Ok(state) => state,
        Err(refusal) => return refusal,
    };
    let session = req
        .head
        .path
        .strip_prefix(DETAIL_PREFIX)
        .unwrap_or_default();
    let Some(draft) = get(req.ctx, &state, session) else {
        return ApiResponse::error(404, "not_found", "no such kickoff draft");
    };
    let view = views(req.ctx, std::slice::from_ref(&draft)).remove(0);
    ApiResponse::json(
        200,
        &KickoffDraftDetail {
            draft: view,
            shape: draft.effective_shape(),
            messages: draft.messages,
        },
    )
}
