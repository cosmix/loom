//! `POST /api/findings/status`: set one ledger entry's `Status:` line.
//!
//! The detail is reader-typed text. A planned detail must be a `PLAN-<slug>.md` token, a
//! folded one must name an entry of the feature's metaplan, a dismissal reason is cut to one
//! line (`findings::reason`); the ledger write itself sanitizes again.

use serde::Deserialize;

use crate::plan_server::files::save::{
    outcome_response, refusal_response, save_document, SaveRequest,
};
use crate::plan_server::request::{ApiRequest, ApiResponse};
use crate::plan_server::wire::paths::to_doc_rel;
use crate::plan_server::wire::trace::{FindingKindView, FindingStatusView};
use crate::planning::discover::read_doc;
use crate::planning::findings::{ledger_path, reason, set_status, FindingStatus};
use crate::planning::kinds::is_feature_name;

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
    let anchors = {
        let project = req.ctx.project();
        project.feature(&body.feature).map(|feature| {
            feature
                .trace
                .entries
                .iter()
                .map(|entry| entry.anchor.clone())
                .collect::<Vec<_>>()
        })
    };
    let Some(anchors) = anchors else {
        return ApiResponse::error(404, "not_found", "no such feature");
    };
    let status = match status_of(&body.status, &anchors) {
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
pub(super) fn status_of(
    view: &FindingStatusView,
    anchors: &[String],
) -> Result<FindingStatus, &'static str> {
    let detail = view.detail.as_deref().map(str::trim);
    match (view.kind, detail) {
        (FindingKindView::Open, None) => Ok(FindingStatus::Open),
        (FindingKindView::Open, Some(_)) => Err("an open status has no detail"),
        (FindingKindView::Planned, Some(token)) if is_plan_token(token) => {
            Ok(FindingStatus::Planned(token.to_owned()))
        }
        (FindingKindView::Planned, _) => Err("a planned status names a plan, PLAN-<slug>.md"),
        (FindingKindView::Folded, Some(token)) if names_entry(token, anchors) => {
            Ok(FindingStatus::Folded(token.to_owned()))
        }
        (FindingKindView::Folded, _) => {
            Err("a folded status names an entry of the metaplan, metaplan#<anchor>")
        }
        (FindingKindView::Dismissed, Some(why)) => reason(why)
            .map(FindingStatus::Dismissed)
            .ok_or("a dismissal needs a reason"),
        (FindingKindView::Dismissed, None) => Err("a dismissal needs a reason"),
    }
}

/// `PLAN-<slug>.md` with a non-empty slug of letters, digits, `-`, `_` and inner dots.
fn is_plan_token(token: &str) -> bool {
    let Some(slug) = token
        .strip_prefix("PLAN-")
        .and_then(|rest| rest.strip_suffix(".md"))
    else {
        return false;
    };
    !slug.is_empty()
        && !slug.starts_with('.')
        && slug
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// `metaplan#<anchor>` for an entry anchor of the feature's metaplan.
fn names_entry(token: &str, anchors: &[String]) -> bool {
    token
        .strip_prefix("metaplan#")
        .is_some_and(|anchor| anchors.iter().any(|known| known == anchor))
}
