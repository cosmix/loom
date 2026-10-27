//! `GET /api/diff/head?path=` and `GET /api/diff/turn?turn=&path=`: the before and after texts
//! of a diff (section 06, "Diffs"). Both are viewer routes and read nothing but the text store
//! and, for the uncommitted diff, git's object database.

use std::path::Path;

use super::doc::{checked_path, discovered, respond, text_store, MAX_EDIT_BYTES};
use super::texts::{PinnedText, TurnId};
use crate::git::runner::run_git;
use crate::plan_server::context::PlanContext;
use crate::plan_server::request::{ApiRequest, ApiResponse};
use crate::plan_server::wire::files::DiffTexts;

/// The uncommitted diff: `HEAD`'s version against the current text.
pub fn head_diff(req: &ApiRequest<'_>) -> ApiResponse {
    respond(head_texts(req.ctx, req.query.get("path")))
}

/// What a turn changed in one file: its text when the turn began against its text at the end.
pub fn turn_diff(req: &ApiRequest<'_>) -> ApiResponse {
    respond(turn_texts(
        req.ctx,
        req.query.get("turn"),
        req.query.get("path"),
    ))
}

/// The texts of the uncommitted diff of `raw` (relative to `doc/`). The path passes the syntax
/// checks (400) and discovery (404) before git runs.
pub(crate) fn head_texts(ctx: &PlanContext, raw: Option<&str>) -> Result<DiffTexts, ApiResponse> {
    let rel = checked_path(raw)?;
    discovered(ctx, &rel)?;
    let store = text_store(ctx)?;
    let before = head_text(ctx.root(), &rel)?;
    let after = store.current(&rel).map(|pinned| pinned.text.to_string());
    Ok(DiffTexts {
        path: rel.to_string_lossy().into_owned(),
        before,
        after,
    })
}

/// `HEAD`'s text of `rel` (repo-relative, syntax-checked and discovered); `None` when `HEAD`
/// has no such file (untracked, or no commit yet).
///
/// Section 12 asks server git calls to pass paths after `--`. The only git read here is
/// `git show HEAD:./<path>`, where the path is part of a revision expression and `--` does not
/// apply. The argument always starts with `HEAD:`, so git never reads it as an option, and the
/// path has passed `check_relpath_syntax` and discovery, so it holds no `..` component. The
/// `./` makes git resolve it against the project root even when that is not the repository's
/// top level. Git reads blobs from its object database, which sessions cannot write, and
/// `show` prints a blob as stored (no textconv).
fn head_text(root: &Path, rel: &Path) -> Result<Option<String>, ApiResponse> {
    let spec = format!("HEAD:./{}", rel.to_string_lossy());
    let output = run_git(&["show", &spec], root).map_err(|error| {
        tracing::warn!("git show {spec} failed to run: {error:#}");
        ApiResponse::error(500, "internal", "git could not be run")
    })?;
    if !output.status.success() {
        return Ok(None);
    }
    if output.stdout.len() > MAX_EDIT_BYTES {
        let message = format!("HEAD's version of {} is over 1 MiB", rel.display());
        return Err(ApiResponse::error(413, "too_large", &message));
    }
    // A diff shows source text; a committed version that is not UTF-8 shows replacement
    // characters where its bytes do not decode.
    Ok(Some(String::from_utf8_lossy(&output.stdout).into_owned()))
}

/// The texts of `turn`'s diff of `raw`. A file the turn created has no `before`, one it deleted
/// no `after`. An unknown turn, or a file the turn left as it was, is 404. The path is only
/// syntax-checked: a file the turn deleted is no longer discovered, and the lookup reads the
/// turn's pins in memory, never the disk.
pub(crate) fn turn_texts(
    ctx: &PlanContext,
    turn: Option<&str>,
    raw: Option<&str>,
) -> Result<DiffTexts, ApiResponse> {
    let turn = match turn {
        Some(turn) if !turn.is_empty() => TurnId(turn.to_owned()),
        _ => {
            return Err(ApiResponse::error(
                400,
                "bad_request",
                "the turn parameter is missing",
            ))
        }
    };
    let rel = checked_path(raw)?;
    let store = text_store(ctx)?;
    let before = store.turn_pin(&turn, &rel);
    let after = store.turn_end(&turn, &rel);
    if hash_of(&before) == hash_of(&after) {
        let message = format!("turn {} did not change {}", turn.0, rel.display());
        return Err(ApiResponse::error(404, "not_found", &message));
    }
    Ok(DiffTexts {
        path: rel.to_string_lossy().into_owned(),
        before: before.map(|pinned| pinned.text.to_string()),
        after: after.map(|pinned| pinned.text.to_string()),
    })
}

fn hash_of(text: &Option<PinnedText>) -> Option<&str> {
    text.as_ref().map(|pinned| pinned.hash.as_str())
}
