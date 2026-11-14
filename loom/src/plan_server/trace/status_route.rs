//! `POST /api/findings/status`: set one ledger entry's `Status:` line.
//!
//! The reader reopens or dismisses a finding here; `addressed` and `unaddressed` are written by
//! the address session that edits the documents, so the route refuses them. A dismissal reason
//! is reader-typed text, cut to one line (`findings::reason`); the ledger write itself sanitizes
//! again.

use serde::Deserialize;

use crate::plan_server::files::save::{
    outcome_response, refusal_response, save_document, SaveOutcome, SaveRequest,
};
use crate::plan_server::request::{ApiRequest, ApiResponse};
use crate::plan_server::wire::paths::to_doc_rel;
use crate::plan_server::wire::trace::{FindingKindView, FindingStatusView};
use crate::planning::discover::read_doc;
use crate::planning::findings::{ledger_path, reason, set_status, FindingStatus};
use crate::planning::kinds::is_feature_name;
use crate::planning::sections::normalized_hash;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StatusRequest {
    feature: String,
    id: String,
    /// The `normalized_hash` of the ledger text the reader opened.
    hash: String,
    status: FindingStatusView,
}

pub fn status_route(req: &ApiRequest<'_>) -> ApiResponse {
    let body: StatusRequest = match req.json() {
        Ok(body) => body,
        Err(response) => return response,
    };
    if !is_feature_name(&body.feature) {
        return ApiResponse::error(400, "bad_request", "the feature name is invalid");
    }
    if req.ctx.project().feature(&body.feature).is_none() {
        return ApiResponse::error(404, "not_found", "no such feature");
    }
    let status = match status_of(&body.status) {
        Ok(status) => status,
        Err(message) => return ApiResponse::error(400, "bad_request", message),
    };
    write_status(req, &body, &status)
}

fn write_status(req: &ApiRequest<'_>, body: &StatusRequest, status: &FindingStatus) -> ApiResponse {
    let rel = ledger_path(&body.feature);
    let Ok(text) = read_doc(req.ctx.root(), &rel) else {
        return ApiResponse::error(404, "not_found", "the feature has no findings ledger");
    };
    // The new text is built from this read, so it must be the text the reader opened: a file that
    // changed away and back since would otherwise take text built from the middle version.
    let current_hash = normalized_hash(&text);
    if current_hash != body.hash {
        return outcome_response(SaveOutcome::Conflict {
            current_text: text,
            current_hash,
        });
    }
    let Ok(new_text) = set_status(&text, &body.id, status) else {
        return ApiResponse::error(400, "bad_request", "the ledger has no such finding");
    };
    let Some(path) = rel.to_str().and_then(to_doc_rel) else {
        return ApiResponse::error(500, "internal", "the ledger path is invalid");
    };
    let save = SaveRequest {
        path,
        text: &new_text,
        opened_hash: &body.hash,
        force: false,
    };
    match save_document(req.ctx, req.browser, save) {
        Ok(outcome) => outcome_response(outcome),
        Err(refusal) => refusal_response(req.ctx, &refusal),
    }
}

/// The ledger status a request asks for, or the reason it is refused.
pub(super) fn status_of(view: &FindingStatusView) -> Result<FindingStatus, &'static str> {
    let detail = view.detail.as_deref().map(str::trim);
    match (view.kind, detail) {
        (FindingKindView::Open, None) => Ok(FindingStatus::Open),
        (FindingKindView::Open, Some(_)) => Err("an open status has no detail"),
        (FindingKindView::Dismissed, Some(why)) => reason(why)
            .map(FindingStatus::Dismissed)
            .ok_or("a dismissal needs a reason"),
        (FindingKindView::Dismissed, None) => Err("a dismissal needs a reason"),
        (FindingKindView::Addressed | FindingKindView::Unaddressed, _) => {
            Err("only an address session marks a finding addressed or not")
        }
    }
}
