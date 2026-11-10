//! Commit exactly the selected documentation changes, with the user's hooks and signing.

use std::path::Path;
use std::sync::{Arc, Weak};
use std::thread;

use anyhow::{ensure, Context, Result};

use crate::git::user_git::{head, literal, user_git, IndexBefore};
use crate::plan_server::context::PlanContext;
use crate::plan_server::request::{ApiRequest, ApiResponse};
use crate::plan_server::wire::run::{ChangedPath, CommitRequest, OutputStream, RunJob};

use super::child;

struct CommitContext(Weak<PlanContext>);

/// Install the weak handle used to move an admitted commit onto its worker thread.
pub(super) fn install(ctx: &Arc<PlanContext>) {
    child::install_jobs(ctx);
    ctx.extensions()
        .insert(Arc::new(CommitContext(Arc::downgrade(ctx))));
}

fn valid_paths(paths: &[String], current: &[ChangedPath]) -> bool {
    // An empty pathspec would give commit access to unrelated staged changes.
    !paths.is_empty()
        && paths.iter().all(|path| {
            path.starts_with("doc/")
                && !Path::new(path).is_absolute()
                && !path.contains('\0')
                && !path.contains("..")
                && path.split('/').all(|part| !matches!(part, "" | "." | ".."))
                && current.iter().any(|changed| changed.path == *path)
        })
}

fn valid_message(message: &str) -> bool {
    // A NUL cannot pass through argv: git commit would fail to spawn after git add staged.
    !message.is_empty() && message.len() <= 2_000 && !message.contains('\0')
}

/// Validate before touching the index or HEAD, and stream both commands' output.
pub fn commit_doc_changes(
    root: &Path,
    paths: &[String],
    message: &str,
    current: &[ChangedPath],
    on_line: &mut dyn FnMut(&str),
) -> anyhow::Result<i32> {
    ensure!(valid_paths(paths, current), "invalid documentation paths");
    ensure!(valid_message(message), "invalid commit message");
    run_sequence(root, paths, message, |_, _, line| on_line(line))
}

/// The ticked paths `git add` can match. A staged deletion or a rename's source is in neither
/// the index nor the worktree, and `git add` exits 128 on such a pathspec; `git commit -- <paths>`
/// still records those paths from HEAD, so they stay on the commit's pathspec only.
fn addable_paths<'a>(root: &Path, paths: &'a [String]) -> Vec<&'a String> {
    paths
        .iter()
        .filter(|path| root.join(path).symlink_metadata().is_ok())
        .collect()
}

fn run_sequence(
    root: &Path,
    paths: &[String],
    message: &str,
    mut on_line: impl FnMut(&str, OutputStream, &str),
) -> Result<i32> {
    let index = IndexBefore::read(root).context("reading the index before the commit")?;
    let addable = addable_paths(root, paths);
    if !addable.is_empty() {
        let mut add = user_git(root);
        add.args(["-c", "core.quotepath=off", "add", "--"])
            .args(literal(&addable));
        let code = run_step(add, "git add", &mut on_line)?;
        if code != 0 {
            return Ok(code);
        }
    }
    let mut commit = user_git(root);
    // Explicit pathspecs give --only semantics, preserving everything else staged.
    commit
        .args(["commit", "-m", message, "--"])
        .args(literal(paths));
    let ran = run_step(commit, "git commit", &mut on_line);
    // HEAD, not the exit status, says whether git made the commit (a `post-commit` hook runs
    // after it exists); HEAD that cannot be read counts as moved. The commit stands whatever the
    // reset does, so what it leaves to know is more lines of output.
    if head(root).map_or(true, |after| after.as_deref() != index.head()) {
        for note in index.reset(root, paths, "HEAD").notes() {
            on_line("git reset", OutputStream::Stderr, &note);
        }
    }
    ran
}

/// Run `cmd`, streaming its lines as step `name`; its exit code, -1 for a signal.
fn run_step(
    cmd: std::process::Command,
    name: &str,
    on_line: &mut impl FnMut(&str, OutputStream, &str),
) -> Result<i32> {
    let status = child::run_streamed(cmd, |stream, line| on_line(name, stream, line))
        .with_context(|| format!("running {name}"))?;
    Ok(status.code().unwrap_or(-1))
}

pub(super) fn post_commit(req: &ApiRequest<'_>) -> ApiResponse {
    if req.ctx.read_only().is_read_only() {
        return ApiResponse::error(403, "read_only", "the server is read-only");
    }
    let body: CommitRequest = match req.json() {
        Ok(body) => body,
        Err(error) => return error,
    };
    if !valid_message(&body.message) {
        return ApiResponse::error(
            400,
            "invalid_message",
            "use a message of 1 to 2,000 bytes with no NUL byte",
        );
    }
    let (current, _) = match super::changes::split_changes(req.ctx.root()) {
        Ok(changes) => changes,
        Err(error) => {
            tracing::warn!("commit could not read current changes: {error:#}");
            return ApiResponse::error(500, "internal", "current changes could not be read");
        }
    };
    if !valid_paths(&body.paths, &current) {
        return ApiResponse::error(400, "invalid_paths", "select current changes under doc/");
    }
    let Some(guard) = child::jobs(req.ctx).begin(RunJob::Commit) else {
        return ApiResponse::error(409, "busy", "another run job is active");
    };
    let ctx = req
        .ctx
        .extensions()
        .get::<CommitContext>()
        .and_then(|ctx| ctx.0.upgrade());
    let Some(ctx) = ctx else {
        return ApiResponse::error(500, "internal", "the commit worker is unavailable");
    };
    spawn_commit(ctx, body, guard)
}

fn spawn_commit(ctx: Arc<PlanContext>, body: CommitRequest, guard: child::JobGuard) -> ApiResponse {
    let worker_ctx = Arc::clone(&ctx);
    let spawned = thread::Builder::new()
        .name("loom-plan-commit".into())
        .spawn(move || {
            let _guard = guard;
            commit_worker(&worker_ctx, body);
        });
    match spawned {
        Ok(worker) => {
            ctx.own_worker("commit", worker, None);
            ApiResponse::json(202, &serde_json::json!({ "accepted": true }))
        }
        Err(error) => {
            tracing::warn!("commit worker could not start: {error}");
            ApiResponse::error(500, "internal", "the commit worker could not start")
        }
    }
}

/// The job's single step label: `commit_doc_changes` streams lines without naming a step.
const COMMIT_STEP: &str = "git commit";

fn commit_worker(ctx: &PlanContext, body: CommitRequest) {
    let root = ctx.root();
    // Recompute the allowlist now: the browser's list may predate other edits.
    let result = super::changes::split_changes(root).and_then(|(current, _)| {
        commit_doc_changes(root, &body.paths, &body.message, &current, &mut |line| {
            child::emit_line(ctx, RunJob::Commit, COMMIT_STEP, OutputStream::Stdout, line);
        })
    });
    let code = match result {
        Ok(code) => code,
        Err(error) => {
            child::emit_line(
                ctx,
                RunJob::Commit,
                COMMIT_STEP,
                OutputStream::Stderr,
                &format!("{error:#}"),
            );
            -1
        }
    };
    child::emit_done(ctx, RunJob::Commit, COMMIT_STEP, code);
}

#[cfg(test)]
mod tests;
