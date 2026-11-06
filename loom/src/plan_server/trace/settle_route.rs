//! `POST /api/settle?path=&hash=[&confirm=1][&clear=1]`: write or remove a document's
//! `Settled:` line (section "Settled marks" of the planning-trace brief).

use std::path::Path;

use serde_json::json;

use crate::plan_server::files::doc::editable;
use crate::plan_server::files::save::{
    outcome_response, refusal_response, save_document, SaveOutcome, SaveRequest,
};
use crate::plan_server::request::{ApiRequest, ApiResponse};
use crate::planning::discover::read_doc;
use crate::planning::paths::check_relpath_syntax;
use crate::planning::project::Project;
use crate::planning::sections::normalized_hash;
use crate::planning::trace::settle;
use crate::planning::trace::{Progress, SettledLine};

/// What the loaded project knows about the document being settled.
struct Target {
    /// The line settling now would write.
    current: SettledLine,
    my_calls: u32,
    open_questions: u32,
    /// False for a locked document: `save_document` refuses it with the lock notice.
    editable: bool,
    /// The hash of the text the trace was computed from.
    loaded_hash: Option<String>,
}

/// The trace document at `rel`; a stub (no file yet) or a path outside the trace has none.
fn target(project: &Project, rel: &Path) -> Option<Target> {
    let doc = project
        .features
        .iter()
        .flat_map(|feature| &feature.trace.documents)
        .find(|doc| doc.path == rel && doc.progress != Progress::Stub)?;
    Some(Target {
        current: doc.current.clone(),
        my_calls: doc.my_calls,
        open_questions: doc.open_questions,
        editable: editable(project, rel),
        loaded_hash: project.content_hash(rel).map(str::to_owned),
    })
}

/// The target at `path` and its text, which must be the text the reader opened: the new text is
/// built from it, and a file that changed away and back between this read and the save would
/// otherwise take text built from the middle version. A mismatch is the conflict response.
fn opened(req: &ApiRequest<'_>, path: &str, hash: &str) -> Result<(Target, String), ApiResponse> {
    let rel = check_relpath_syntax(path)
        .map_err(|error| ApiResponse::error(400, "bad_request", &error.to_string()))?;
    let Some(target) = target(&req.ctx.project(), &rel) else {
        return Err(ApiResponse::error(
            400,
            "bad_request",
            "only the vision, the sections, the metaplan and the plans can be settled",
        ));
    };
    let Ok(text) = read_doc(req.ctx.root(), &rel) else {
        return Err(ApiResponse::error(
            404,
            "not_found",
            "the document could not be read",
        ));
    };
    let current_hash = normalized_hash(&text);
    if current_hash != hash {
        return Err(outcome_response(SaveOutcome::Conflict {
            current_text: text,
            current_hash,
        }));
    }
    Ok((target, text))
}

pub fn settle_route(req: &ApiRequest<'_>) -> ApiResponse {
    let (Some(path), Some(hash)) = (req.query.get("path"), req.query.get("hash")) else {
        return ApiResponse::error(
            400,
            "bad_request",
            "the path and hash parameters are required",
        );
    };
    let (target, text) = match opened(req, path, hash) {
        Ok(opened) => opened,
        Err(response) => return response,
    };
    let clear = req.query.get("clear") == Some("1");
    if target.editable && !clear {
        if let Some(refusal) = refuse(req, &target, hash) {
            return refusal;
        }
    }
    let new_text = if clear {
        settle::clear(&text)
    } else {
        settle::mark(&text, &target.current)
    };
    let save = SaveRequest {
        path,
        text: &new_text,
        opened_hash: hash,
        force: false,
    };
    match save_document(req.ctx, req.browser, save) {
        Ok(outcome) => outcome_response(outcome),
        Err(refusal) => refusal_response(req.ctx, &refusal),
    }
}

/// 409 when the trace has not caught up with the text the reader opened (the line it would
/// write describes an older text), or when the document holds points the reader has not
/// confirmed.
fn refuse(req: &ApiRequest<'_>, target: &Target, hash: &str) -> Option<ApiResponse> {
    if target.loaded_hash.as_deref() != Some(hash) {
        return Some(ApiResponse::error(
            409,
            "stale",
            "the planning view is catching up with the document; try again in a moment",
        ));
    }
    let open = target.my_calls > 0 || target.open_questions > 0;
    if !open || req.query.get("confirm") == Some("1") {
        return None;
    }
    Some(ApiResponse::json(
        409,
        &json!({
            "error": "unresolved",
            "message": "the document holds unresolved points; confirm to settle it anyway",
            "my_calls": target.my_calls,
            "open_questions": target.open_questions,
        }),
    ))
}
