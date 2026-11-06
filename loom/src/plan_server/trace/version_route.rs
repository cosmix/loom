//! `GET /api/settled-version?path=`: the commit holding a document's current `Settled:` line.

use std::path::Path;

use serde::Serialize;

use crate::git::runner::run_git;
use crate::plan_server::request::{ApiRequest, ApiResponse};
use crate::plan_server::review::compare::{is_commit_id, text_at};
use crate::planning::discover::read_doc;
use crate::planning::paths::{check_relpath_syntax, symlink_component};
use crate::planning::trace::settle;

/// The answer: `commit` is null when the line was never committed or git finds nothing.
#[derive(Serialize)]
struct SettledVersion {
    commit: Option<String>,
}

pub fn version_route(req: &ApiRequest<'_>) -> ApiResponse {
    let Some(path) = req.query.get("path") else {
        return ApiResponse::error(400, "bad_request", "the path parameter is required");
    };
    let rel = match check_relpath_syntax(path) {
        Ok(rel) => rel,
        Err(error) => return ApiResponse::error(400, "bad_request", &error.to_string()),
    };
    let root = req.ctx.root();
    match symlink_component(root, &rel) {
        Ok(None) => {}
        Ok(Some(_)) => {
            return ApiResponse::error(400, "bad_path", "the document path crosses a symlink")
        }
        Err(error) => {
            tracing::warn!("loom plan could not check a document path for symlinks: {error:#}");
            return ApiResponse::error(500, "io", "the document path could not be checked");
        }
    }
    // Copy out of the project and drop the read guard before git runs: the watcher waits on it.
    if req.ctx.project().tree.get(&rel).is_none() {
        return ApiResponse::error(404, "not_found", "no such document");
    }
    let line = read_doc(root, &rel)
        .ok()
        .and_then(|text| settle::settled_line(&text))
        .map(|line| settle::render(&line));
    let commit = line.and_then(|line| holding_commit(root, path, &rel, &line));
    ApiResponse::json(200, &SettledVersion { commit })
}

/// The newest commit that changed the number of occurrences of `line` in `rel`, when that
/// commit's version of the file holds the line; `None` when git finds nothing or fails.
fn holding_commit(root: &Path, doc_rel: &str, rel: &Path, line: &str) -> Option<String> {
    let pickaxe = format!("-S{line}");
    let target = rel.to_str()?;
    let args = [
        "--literal-pathspecs",
        "log",
        pickaxe.as_str(),
        "-1",
        "--format=%H",
        "--",
        target,
    ];
    let output = run_git(&args, root).ok().filter(|o| o.status.success())?;
    let commit = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if !is_commit_id(&commit) {
        return None;
    }
    // The pickaxe also finds the commit that removed the line; keep only a commit that has it.
    let text = text_at(root, &commit, doc_rel).ok().flatten()?;
    text.lines().any(|found| found == line).then_some(commit)
}
