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

/// How many pickaxe commits (newest first) are checked for the line: a long history cannot stall
/// the request. Raise it if settled documents routinely outlive twenty changes to their mark.
const MAX_COMMITS: usize = 20;

/// The newest commit whose version of `rel` holds `line`, among the newest [`MAX_COMMITS`]
/// commits that changed the number of occurrences of `line`; `None` when git finds none or fails.
fn holding_commit(root: &Path, doc_rel: &str, rel: &Path, line: &str) -> Option<String> {
    let pickaxe = format!("-S{line}");
    let target = rel.to_str()?;
    let args = [
        "--literal-pathspecs",
        "log",
        "--no-show-signature",
        pickaxe.as_str(),
        "--format=%H",
        "--",
        target,
    ];
    let output = run_git(&args, root).ok().filter(|o| o.status.success())?;
    let listing = String::from_utf8_lossy(&output.stdout).into_owned();
    // The pickaxe also lists commits that removed the line; keep the first that has it.
    listing
        .lines()
        .map(str::trim)
        .filter(|commit| is_commit_id(commit))
        .take(MAX_COMMITS)
        .find(|commit| {
            text_at(root, commit, doc_rel)
                .ok()
                .flatten()
                .is_some_and(|text| text.lines().any(|found| found == line))
        })
        .map(str::to_owned)
}
