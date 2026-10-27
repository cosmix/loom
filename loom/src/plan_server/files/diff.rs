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
/// checks (400) and discovery (404) before git runs. A side over 1 MiB is 413: doc_view cuts
/// such a document, and a diff of a cut text would show deletions that are not there.
pub(crate) fn head_texts(ctx: &PlanContext, raw: Option<&str>) -> Result<DiffTexts, ApiResponse> {
    let rel = checked_path(raw)?;
    discovered(ctx, &rel)?;
    let store = text_store(ctx)?;
    let before = head_text(ctx.root(), &rel)?;
    let after = bounded(store.current(&rel), "the current version", &rel)?;
    Ok(DiffTexts {
        path: rel.to_string_lossy().into_owned(),
        before,
        after,
    })
}

/// `HEAD`'s text of `rel` (repo-relative, syntax-checked and discovered); `None` when `HEAD`
/// has no such file (untracked, or no commit yet).
///
/// Section 12 asks server git calls to pass paths after `--`. The only git reads of the path
/// are `git rev-parse --verify --quiet HEAD:./<path>`, where the path is part of a revision
/// expression and `--` does not apply. The argument always starts with `HEAD:`, so git never
/// reads it as an option, and the path has passed `check_relpath_syntax` and discovery, so it
/// holds no `..` component. The `./` makes git resolve it against the project root even when
/// that is not the repository's top level. Git reads blobs from its object database, which
/// sessions cannot write, and `cat-file blob` prints a blob as stored (no textconv).
///
/// The exit status of `rev-parse` tells "not in HEAD" (1) from every other failure. Its object
/// id is then asked for the blob's size before the blob is read, so a large blob is never
/// buffered (`run_git` bounds time, not output), and the size and the content come from one
/// object even if `HEAD` moves in between.
fn head_text(root: &Path, rel: &Path) -> Result<Option<String>, ApiResponse> {
    let spec = format!("HEAD:./{}", rel.to_string_lossy());
    let Some(blob) = head_blob(root, &spec)? else {
        return Ok(None);
    };
    let size = git_stdout(root, &["cat-file", "-s", &blob])?;
    let size: usize = String::from_utf8_lossy(&size)
        .trim()
        .parse()
        .map_err(|_| git_failed("git cat-file -s printed no size"))?;
    if size > MAX_EDIT_BYTES {
        return Err(too_large("HEAD's version", rel));
    }
    let bytes = git_stdout(root, &["cat-file", "blob", &blob])?;
    // A diff shows source text; a committed version that is not UTF-8 shows replacement
    // characters where its bytes do not decode.
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

/// The object id of `spec` in `HEAD`; `None` when `HEAD` has no such path (or no commit).
fn head_blob(root: &Path, spec: &str) -> Result<Option<String>, ApiResponse> {
    let output = run_git(&["rev-parse", "--verify", "--quiet", spec], root).map_err(git_failed)?;
    match output.status.code() {
        Some(0) => {
            let id = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            if id.is_empty() || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                tracing::warn!("git rev-parse {spec} printed no object id");
                return Err(internal_git());
            }
            Ok(Some(id))
        }
        Some(1) => Ok(None),
        _ => Err(git_failed(format!(
            "git rev-parse {spec}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))),
    }
}

/// The stdout of a git read that must succeed.
fn git_stdout(root: &Path, args: &[&str]) -> Result<Vec<u8>, ApiResponse> {
    let output = run_git(args, root).map_err(git_failed)?;
    if output.status.success() {
        return Ok(output.stdout);
    }
    Err(git_failed(format!(
        "git {}: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr).trim()
    )))
}

/// 500 with a generic message; the detail goes to the log, never to the browser.
fn git_failed(detail: impl std::fmt::Display) -> ApiResponse {
    tracing::warn!("file mode diff: {detail:#}");
    internal_git()
}

fn internal_git() -> ApiResponse {
    ApiResponse::error(500, "internal", "git could not read HEAD's version")
}

fn too_large(what: &str, rel: &Path) -> ApiResponse {
    let message = format!("{what} of {} is over 1 MiB", rel.display());
    ApiResponse::error(413, "too_large", &message)
}

/// The text of `pinned` for the wire, or 413 when it is over 1 MiB.
fn bounded(
    pinned: Option<PinnedText>,
    what: &str,
    rel: &Path,
) -> Result<Option<String>, ApiResponse> {
    match pinned {
        Some(pinned) if pinned.text.len() > MAX_EDIT_BYTES => Err(too_large(what, rel)),
        Some(pinned) => Ok(Some(pinned.text.to_string())),
        None => Ok(None),
    }
}

/// The texts of `turn`'s diff of `raw`. A file the turn created has no `before`, one it deleted
/// no `after`. An unknown turn, a turn that has not ended, or a file the turn left as it was is
/// 404. The path is only syntax-checked: a file the turn deleted is no longer discovered, and
/// the lookup reads the turn's pins in memory, never the disk. A side over 1 MiB is 413, as in
/// the uncommitted diff.
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
    // Without an end every pinned file would read as deleted.
    if !store.turn_ended(&turn) {
        let message = format!("turn {} is unknown or has not ended", turn.0);
        return Err(ApiResponse::error(404, "not_found", &message));
    }
    let before = store.turn_pin(&turn, &rel);
    let after = store.turn_end(&turn, &rel);
    if hash_of(&before) == hash_of(&after) {
        let message = format!("turn {} did not change {}", turn.0, rel.display());
        return Err(ApiResponse::error(404, "not_found", &message));
    }
    Ok(DiffTexts {
        path: rel.to_string_lossy().into_owned(),
        before: bounded(before, "the version before the turn", &rel)?,
        after: bounded(after, "the version after the turn", &rel)?,
    })
}

fn hash_of(text: &Option<PinnedText>) -> Option<&str> {
    text.as_ref().map(|pinned| pinned.hash.as_str())
}
