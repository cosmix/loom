//! `POST /api/settle?path=&hash=[&confirm=1][&clear=1]`: write or remove a document's
//! `Settled:` line (section "Settled marks" of the planning-trace brief), then queue its commit
//! (`settle_queue`) and answer at once: the commit's end arrives as a `settle_commit` message.

use std::path::{Path, PathBuf};

use serde_json::json;

use crate::plan_server::files::doc::editable;
use crate::plan_server::files::save::{
    outcome_response, refusal_response, save_document, SaveOutcome, SaveRequest,
};
use crate::plan_server::request::{ApiRequest, ApiResponse};
use crate::plan_server::wire::files::ValidationView;
use crate::plan_server::wire::trace::SettleResult;
use crate::planning::discover::read_doc;
use crate::planning::paths::check_relpath_syntax;
use crate::planning::project::Project;
use crate::planning::sections::normalized_hash;
use crate::planning::trace::settle;
use crate::planning::trace::{Progress, SettledLine};

use super::settle_job::{BriefSeen, SettleJob};
use super::settle_queue;

/// What the loaded project knows about the document being settled.
struct Target {
    /// The feature whose trace holds it.
    feature: String,
    /// A plan's brief files, which its settled hash covers.
    bundle: Vec<PathBuf>,
    /// The hash of the project's text of each brief, which the line's body hash covers; `None`
    /// for a brief it holds no text for.
    brief_texts: Vec<Option<String>>,
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
    let (feature, doc) = project.features.iter().find_map(|feature| {
        let doc = feature
            .trace
            .documents
            .iter()
            .find(|doc| doc.path == rel && doc.progress != Progress::Stub)?;
        Some((feature, doc))
    })?;
    let brief_texts = doc
        .bundle
        .iter()
        .map(|brief| project.text(brief).map(normalized_hash))
        .collect();
    Some(Target {
        feature: feature.name.clone(),
        bundle: doc.bundle.clone(),
        brief_texts,
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
fn opened(
    req: &ApiRequest<'_>,
    path: &str,
    hash: &str,
) -> Result<(PathBuf, Target, String), ApiResponse> {
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
    Ok((rel, target, text))
}

pub fn settle_route(req: &ApiRequest<'_>) -> ApiResponse {
    let (Some(path), Some(hash)) = (req.query.get("path"), req.query.get("hash")) else {
        return ApiResponse::error(
            400,
            "bad_request",
            "the path and hash parameters are required",
        );
    };
    let (rel, target, text) = match opened(req, path, hash) {
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
        Ok(SaveOutcome::Saved { hash, validation }) => {
            let job = settle_job(req, rel, target, clear, &hash);
            queued(req, job, hash, validation)
        }
        Ok(outcome) => outcome_response(outcome),
        Err(refusal) => refusal_response(req.ctx, &refusal),
    }
}

/// The commit of the line `req` wrote (`written`, its text's hash) over the document at `rel`.
/// The briefs are taken now, as the line's body hash covered them.
fn settle_job(
    req: &ApiRequest<'_>,
    rel: PathBuf,
    target: Target,
    clear: bool,
    written: &str,
) -> SettleJob {
    let root = req.ctx.root();
    let seen = target
        .bundle
        .iter()
        .zip(target.brief_texts)
        .map(|(brief, text)| BriefSeen::of(root, brief, text))
        .collect();
    SettleJob {
        browser: req.browser.clone(),
        feature: target.feature,
        doc: rel,
        briefs: target.bundle,
        seen,
        clear,
        written: written.to_owned(),
    }
}

/// The written line's answer, once `job` is queued: the commit has not started yet.
fn queued(
    req: &ApiRequest<'_>,
    job: SettleJob,
    hash: String,
    validation: Option<ValidationView>,
) -> ApiResponse {
    let commit = settle_queue::push(req.ctx, job);
    let answer = SettleResult::Saved {
        hash,
        validation,
        commit,
    };
    ApiResponse::json(200, &answer)
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
