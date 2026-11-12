//! Bulk settling shares the individual route's checks, saves and per-document commit queue.

use crate::plan_server::request::{ApiRequest, ApiResponse};
use crate::plan_server::wire::paths::to_doc_rel;
use crate::plan_server::wire::trace::{SettleAllResult, SettleSkipReason, SettleSkipped};
use crate::planning::trace::{DocRole, SettleState, TraceDoc};

use super::settle_route::{opened, settle_one};

pub(super) fn role(value: Option<&str>) -> Option<DocRole> {
    match value? {
        "section" => Some(DocRole::Section),
        "plan" => Some(DocRole::Plan),
        _ => None,
    }
}

pub(super) fn needs_settling(doc: &TraceDoc, role: DocRole) -> bool {
    doc.role == role && doc.settleable && doc.settle != SettleState::Settled
}

/// Settle sections before plans: each request accepts exactly one role, so the caller settles
/// sections in one call before requesting plans in another. Refusals never stop later documents.
pub fn settle_all_route(req: &ApiRequest<'_>) -> ApiResponse {
    let (Some(feature), Some(role)) = (req.query.get("feature"), role(req.query.get("role")))
    else {
        return ApiResponse::error(
            400,
            "bad_request",
            "a feature and role section or plan are required",
        );
    };
    // Release the project read guard before saving: the save path reads it again, and the
    // watcher may be waiting to replace it between documents.
    let documents: Vec<_> = {
        let project = req.ctx.project();
        let Some(feature) = project.feature(feature) else {
            return ApiResponse::error(400, "bad_request", "no such feature");
        };
        feature
            .trace
            .documents
            .iter()
            .filter(|doc| needs_settling(doc, role))
            .map(|doc| {
                (
                    doc.path.to_string_lossy().into_owned(),
                    project.content_hash(&doc.path).map(str::to_owned),
                )
            })
            .collect()
    };
    let confirm = req.query.get("confirm") == Some("1");
    let mut result = SettleAllResult {
        settled: Vec::new(),
        skipped: Vec::new(),
    };
    for (path, hash) in documents {
        match settle_document(req, &path, hash.as_deref(), confirm) {
            Ok(()) => result.settled.push(path),
            Err(reason) => result.skipped.push(SettleSkipped { path, reason }),
        }
    }
    ApiResponse::json(200, &result)
}

fn settle_document(
    req: &ApiRequest<'_>,
    path: &str,
    hash: Option<&str>,
    confirm: bool,
) -> Result<(), SettleSkipReason> {
    let hash = hash.ok_or(SettleSkipReason::Stub)?;
    let rel = to_doc_rel(path).ok_or(SettleSkipReason::Stale)?;
    // `opened` compares the loaded content hash with a fresh read, before the shared path
    // verifies that the trace describes that same text. A watcher lag must never write a mark.
    let (rel, target, text) = opened(req, rel, hash).map_err(|_| SettleSkipReason::Stale)?;
    settle_one(req, rel, target, &text, confirm).map_err(|refusal| refusal.reason)?;
    Ok(())
}
