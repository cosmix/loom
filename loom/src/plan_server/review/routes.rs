//! HTTP routes of the review loop.

use std::path::Path;
use std::sync::Arc;

use crate::git::runner::run_git;
use crate::plan_server::concept::links::implemented_by as section_plans;
use crate::plan_server::context::PlanContext;
use crate::plan_server::request::{ApiRequest, ApiResponse, QueryParams};
use crate::plan_server::routes::{BodyKind, Method, Role, RouteSpec};
use crate::plan_server::wire::paths::to_doc_rel;
use crate::plan_server::wire::review::{
    AtCommitResponse, BuiltNoteWire, BuiltResponse, ChangedDocWire, CompareResponse, DocChange,
    HashesResponse, ImplementedByResponse, OpenQuestionsMessage, RefResponse,
};
use crate::planning::paths::{check_relpath_syntax, symlink_component};

use super::built::{built_from, built_inputs, BuiltNote};
use super::compare::{
    changed_since, commit_exists, is_commit_id, resolve_ref, text_at, ChangedDoc, ChangedDocChange,
    RefError,
};
use super::questions::open_question_counts;

pub fn routes() -> Vec<RouteSpec> {
    let viewer = |path, handler: fn(&ApiRequest<'_>) -> ApiResponse| RouteSpec {
        method: Method::Get,
        path,
        body: BodyKind::None,
        role: Role::Viewer,
        handler,
    };
    vec![
        RouteSpec {
            method: Method::Post,
            path: "/api/restore",
            body: BodyKind::Json,
            role: Role::Holder,
            handler: super::turn_routes::restore,
        },
        viewer("/api/turn-sections", super::turn_routes::turn_sections),
        viewer("/api/hashes", hashes),
        viewer("/api/ref", reference),
        viewer("/api/compare", compare),
        viewer("/api/at-commit", at_commit),
        viewer("/api/built", built),
        viewer("/api/open-questions", open_questions),
        viewer("/api/implemented-by", implemented_by),
    ]
}

pub fn install(ctx: &Arc<PlanContext>) {
    super::setup::install(ctx);
}

fn hashes(req: &ApiRequest<'_>) -> ApiResponse {
    // Copy the hashes out and drop the guard before git runs: the watcher waits on it.
    let docs = {
        let project = req.ctx.project();
        project
            .docs
            .keys()
            .filter_map(|path| {
                project
                    .content_hash(path)
                    .map(|hash| (path.to_string_lossy().into_owned(), hash.to_owned()))
            })
            .collect()
    };
    let head = match head_commit(req.ctx.root()) {
        Ok(head) => head,
        Err(error) => return error,
    };
    ApiResponse::json(200, &HashesResponse { head, docs })
}

fn head_commit(root: &Path) -> Result<Option<String>, ApiResponse> {
    let output = run_git(&["rev-parse", "--verify", "--quiet", "HEAD"], root)
        .map_err(|_| ApiResponse::error(500, "internal", "the current commit could not be read"))?;
    if !output.status.success() {
        return Ok(None);
    }
    let commit = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if !is_commit_id(&commit) {
        return Err(ApiResponse::error(
            500,
            "internal",
            "the current commit is invalid",
        ));
    }
    Ok(Some(commit))
}

fn reference(req: &ApiRequest<'_>) -> ApiResponse {
    let name = match parameter(&req.query, "name") {
        Ok(name) => name,
        Err(error) => return error,
    };
    match resolve_ref(req.ctx.root(), name) {
        Ok(commit) => ApiResponse::json(200, &RefResponse { commit }),
        Err(RefError::Invalid(_)) => ApiResponse::error(400, "bad_ref", "the ref name is invalid"),
        Err(RefError::Unknown(_)) => {
            ApiResponse::error(404, "not_found", "the ref does not name a known commit")
        }
    }
}

fn compare(req: &ApiRequest<'_>) -> ApiResponse {
    let commit = match commit_parameter(&req.query) {
        Ok(commit) => commit,
        Err(error) => return error,
    };
    let root = req.ctx.root();
    let available = commit_exists(root, commit);
    let docs = if available {
        match changed_since(root, commit) {
            Ok(docs) => docs.into_iter().map(changed_doc_wire).collect(),
            Err(_) => {
                return ApiResponse::error(500, "internal", "documents could not be compared");
            }
        }
    } else {
        Vec::new()
    };
    ApiResponse::json(
        200,
        &CompareResponse {
            commit: commit.to_owned(),
            available,
            docs,
        },
    )
}

fn changed_doc_wire(doc: ChangedDoc) -> ChangedDocWire {
    ChangedDocWire {
        path: doc.path,
        change: match doc.change {
            ChangedDocChange::Added => DocChange::Added,
            ChangedDocChange::Changed => DocChange::Changed,
            ChangedDocChange::Removed => DocChange::Removed,
        },
    }
}

fn at_commit(req: &ApiRequest<'_>) -> ApiResponse {
    let commit = match commit_parameter(&req.query) {
        Ok(commit) => commit,
        Err(error) => return error,
    };
    let path = match parameter(&req.query, "path") {
        Ok(path) => path,
        Err(error) => return error,
    };
    let Some(doc_rel) = to_doc_rel(path) else {
        return ApiResponse::error(400, "bad_path", "the path must be a repository document");
    };
    let root = req.ctx.root();
    if let Err(error) = validate_doc_path(root, doc_rel) {
        return error;
    }
    if !commit_exists(root, commit) {
        return ApiResponse::error(400, "bad_commit", "the commit is unavailable");
    }
    match text_at(root, commit, doc_rel) {
        Ok(text) => ApiResponse::json(
            200,
            &AtCommitResponse {
                path: path.to_owned(),
                text,
            },
        ),
        Err(_) => ApiResponse::error(500, "internal", "the committed document could not be read"),
    }
}

fn built(req: &ApiRequest<'_>) -> ApiResponse {
    let section = match parameter(&req.query, "section") {
        Ok(section) => section,
        Err(error) => return error,
    };
    if let Err(error) = validate_doc_path(req.ctx.root(), section) {
        return error;
    }
    // The guard covers only the project reads; git runs after it is dropped.
    let inputs = built_inputs(&req.ctx.project(), section);
    match inputs.and_then(built_from) {
        Ok(notes) => ApiResponse::json(
            200,
            &BuiltResponse {
                notes: notes.into_iter().map(built_note_wire).collect(),
            },
        ),
        Err(_) => ApiResponse::error(500, "internal", "built notes could not be read"),
    }
}

fn built_note_wire(note: BuiltNote) -> BuiltNoteWire {
    match note {
        BuiltNote::ChangedAfter {
            slug,
            built,
            base_commit,
        } => BuiltNoteWire::ChangedAfter {
            slug,
            built,
            base_commit,
        },
        BuiltNote::Running { slug } => BuiltNoteWire::Running { slug },
    }
}

fn open_questions(req: &ApiRequest<'_>) -> ApiResponse {
    ApiResponse::json(
        200,
        &OpenQuestionsMessage {
            counts: open_question_counts(&req.ctx.project()),
        },
    )
}

fn implemented_by(req: &ApiRequest<'_>) -> ApiResponse {
    let project = req.ctx.project();
    let sections = project
        .implemented_by
        .keys()
        .map(|&number| (number, section_plans(&project, number)))
        .collect();
    ApiResponse::json(200, &ImplementedByResponse { sections })
}

fn parameter<'a>(query: &'a QueryParams, name: &str) -> Result<&'a str, ApiResponse> {
    query.get(name).ok_or_else(|| {
        ApiResponse::error(
            400,
            "bad_request",
            &format!("the {name} parameter is missing"),
        )
    })
}

fn commit_parameter(query: &QueryParams) -> Result<&str, ApiResponse> {
    let commit = parameter(query, "commit")?;
    if !is_commit_id(commit) {
        return Err(ApiResponse::error(
            400,
            "bad_commit",
            "the commit must be 40 lowercase hex digits",
        ));
    }
    Ok(commit)
}

fn validate_doc_path(root: &Path, raw: &str) -> Result<(), ApiResponse> {
    let rel = check_relpath_syntax(raw)
        .map_err(|_| ApiResponse::error(400, "bad_path", "the document path is invalid"))?;
    match symlink_component(root, &rel) {
        Ok(None) => Ok(()),
        Ok(Some(_)) => Err(ApiResponse::error(
            400,
            "bad_path",
            "the document path crosses a symlink",
        )),
        Err(_) => Err(ApiResponse::error(
            500,
            "internal",
            "the document path could not be checked",
        )),
    }
}
