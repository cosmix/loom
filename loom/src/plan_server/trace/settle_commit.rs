//! The git side of a settle commit, which the settle worker (`settle_queue`) runs once the line is
//! written: commit the settled paths and say what stopped it.
//!
//! Only the settled paths are committed (`git commit --only`): whatever else the user has staged
//! or modified stays as it was. The commit is the user's own: plain `git commit` on the branch
//! checked out, their identity, signing and hooks, never `--no-verify`. Loom drives projects whose
//! hooks it does not know, so nothing here assumes what they do: they may run for minutes, rewrite
//! the files, refuse, or commit themselves. Git's exit status is not trusted to say whether the
//! commit was made (`settle_head`).

use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::Duration;

use anyhow::{ensure, Context, Result};

use crate::git::runner::run_git;
use crate::git::user_git::{literal, run_literal, user_git, IndexBefore};
use crate::process::{run_bounded_terminating_output, ProcessTimeoutError};

use super::settle_head::{after_commit, unstage, Made};

/// How long one commit may take, hooks included. It runs on the settle worker, not the request,
/// so a hook may take minutes (a test suite, a formatter over the repository); past this it is
/// stopped and the reader told why. Revisit if projects' hooks routinely run longer.
const COMMIT_TIMEOUT: Duration = Duration::from_secs(10 * 60);
/// How long git has after `SIGTERM` to remove its `index.lock` and exit before the `SIGKILL`.
const TERM_GRACE: Duration = Duration::from_secs(5);

/// How much of git's refusal reaches the reader: its last lines, cut to this many bytes.
const REFUSAL_LINES: usize = 6;
const REFUSAL_BYTES: usize = 600;

/// The bounds of one git commit the settle worker runs.
#[derive(Debug, Clone, Copy)]
pub(super) struct Limits {
    pub timeout: Duration,
    pub grace: Duration,
}

impl Limits {
    pub(super) const STANDARD: Limits = Limits {
        timeout: COMMIT_TIMEOUT,
        grace: TERM_GRACE,
    };
}

/// What committing a settle came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Recorded {
    /// Git made the commit. `commit` is the settle's own commit, `None` when HEAD no longer holds
    /// it where the settle can tell it apart; `note` is what is left for the reader, as sentences:
    /// git ended badly after making it, HEAD did not show it, the index was not reset to it, or a
    /// hook also committed paths the user had staged.
    Committed {
        commit: Option<String>,
        note: Option<String>,
    },
    /// The paths already matched HEAD, so there was nothing to commit.
    Unchanged,
    /// Why nothing was committed: a repository state that rules a commit out (no repository, a
    /// merge or rebase in progress), git's refusal, a timeout, or git failing to run.
    NotCommitted(String),
}

/// What one settle commits.
pub(super) struct SettleCommit<'a> {
    pub feature: &'a str,
    /// The document, repository-relative (`doc/...`).
    pub doc: &'a Path,
    /// A plan's brief files: the settled hash covers them, so a settle commits them with the plan.
    /// An unsettle records no claim about them and commits the document alone.
    pub briefs: &'a [PathBuf],
    pub clear: bool,
}

impl SettleCommit<'_> {
    fn message(&self) -> String {
        let verb = if self.clear { "unsettle" } else { "settle" };
        let shown = self.doc.strip_prefix("doc").unwrap_or(self.doc);
        format!("docs({}): {verb} {}", self.feature, shown.display())
    }

    /// The committed files: the document, then a settle's briefs.
    pub(super) fn files(&self) -> Vec<&Path> {
        let briefs: &[PathBuf] = if self.clear { &[] } else { self.briefs };
        std::iter::once(self.doc)
            .chain(briefs.iter().map(PathBuf::as_path))
            .collect()
    }

    fn paths(&self) -> Result<Vec<&str>> {
        self.files()
            .into_iter()
            .map(|path| {
                path.to_str()
                    .with_context(|| format!("{} is not UTF-8", path.display()))
            })
            .collect()
    }
}

/// Commit `commit`'s paths in the repository holding `root`. Never fails: an error is reported as
/// [`Recorded::NotCommitted`], because the line is written whatever git does.
pub(super) fn record(root: &Path, commit: &SettleCommit<'_>, limits: Limits) -> Recorded {
    try_record(root, commit, limits).unwrap_or_else(|error| {
        tracing::warn!("loom plan could not commit a settle: {error:#}");
        Recorded::NotCommitted(format!("git could not run: {}", error.root_cause()))
    })
}

/// The bytes commit `sha` holds at `path` (`root`-relative), `None` when it holds no such file.
/// `Err` when git could not answer.
pub(super) fn committed_bytes(root: &Path, sha: &str, path: &Path) -> Result<Option<Vec<u8>>> {
    let path = path
        .to_str()
        .with_context(|| format!("{} is not UTF-8", path.display()))?;
    let output = run_git(&["cat-file", "blob", &format!("{sha}:./{path}")], root)?;
    Ok(output.status.success().then_some(output.stdout))
}

/// Up to `git add` nothing has changed, so an error ends the settle there; from it on, every
/// outcome, a failed `git add` included, unstages what was added and no commit holds
/// ([`after_commit`]).
fn try_record(root: &Path, commit: &SettleCommit<'_>, limits: Limits) -> Result<Recorded> {
    let git_dir = match committable(root)? {
        Ok(git_dir) => git_dir,
        Err(reason) => return Ok(Recorded::NotCommitted(reason)),
    };
    let paths = commit.paths()?;
    if !differs_from_head(root, &paths)? {
        return Ok(Recorded::Unchanged);
    }
    let index = IndexBefore::read(root)?;
    // `commit --only` takes only paths git knows: a new brief is added first.
    let added = untracked(root, &paths)?;
    if !added.is_empty() {
        let refused = match run_literal(root, &["add", "--"], &added) {
            Ok(output) if output.status.success() => None,
            Ok(output) => Some(refusal("git add", &output)),
            Err(error) => Some(format!("git add could not run: {}", error.root_cause())),
        };
        if let Some(reason) = refused {
            return Ok(Recorded::NotCommitted(unstage(root, &added, reason)));
        }
    }
    let message = commit.message();
    let ran = run_commit(root, &git_dir, &message, &paths, limits);
    let made = Made {
        index: &index,
        message: &message,
        paths: &paths,
        added: &added,
    };
    Ok(after_commit(root, &made, ran))
}

/// Run the user's `git commit` of `paths` under `limits`. `Err` says why it did not end in
/// success: git's refusal, a timeout, an output past the limit or git failing to start. Git may
/// have made the commit all the same ([`after_commit`]).
fn run_commit(
    root: &Path,
    git_dir: &Path,
    message: &str,
    paths: &[&str],
    limits: Limits,
) -> Result<(), String> {
    let mut git = user_git(root);
    git.args(["commit", "--only", "--quiet", "-m", message, "--"])
        .args(literal(paths));
    let step = "git commit";
    match run_bounded_terminating_output(&mut git, limits.timeout, limits.grace, step) {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => Err(refusal(step, &output)),
        Err(error) if error.downcast_ref::<ProcessTimeoutError>().is_some() => {
            Err(timed_out(root, git_dir, &error))
        }
        Err(error) => {
            tracing::warn!("loom plan could not run a settle's git commit: {error:#}");
            Err(format!("{step} failed: {}", error.root_cause()))
        }
    }
}

/// Why a commit stopped at its deadline, and whether git's index lock outlived it.
fn timed_out(root: &Path, git_dir: &Path, error: &anyhow::Error) -> String {
    let mut reason = format!("{error} and was stopped");
    let lock = git_dir.join("index.lock");
    if lock.symlink_metadata().is_ok() {
        let shown = lock.strip_prefix(root).unwrap_or(Path::new("index.lock"));
        reason.push_str(&format!(
            "; {} remains, so git refuses other commits until it is removed",
            shown.display()
        ));
    }
    reason
}

/// The git directory of the repository holding `root`, or why it cannot take a commit now:
/// `root` is in no repository, or a merge, rebase, cherry-pick or revert is in progress (its
/// commit would take this one in).
fn committable(root: &Path) -> Result<Result<PathBuf, String>> {
    let output = run_git(&["rev-parse", "--absolute-git-dir"], root)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Any other refusal (a repository git does not trust, say) is git's to explain.
        return Ok(Err(if stderr.contains("not a git repository") {
            "this project is not in a git repository".to_owned()
        } else {
            refusal("git rev-parse", &output)
        }));
    }
    let git_dir = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    let in_progress = [
        ("MERGE_HEAD", "a merge"),
        ("rebase-merge", "a rebase"),
        ("rebase-apply", "a rebase"),
        ("CHERRY_PICK_HEAD", "a cherry-pick"),
        ("REVERT_HEAD", "a revert"),
    ];
    let blocked = in_progress
        .into_iter()
        .find(|(marker, _)| git_dir.join(marker).symlink_metadata().is_ok())
        .map(|(_, what)| format!("{what} is in progress; commit the document once it is done"));
    Ok(blocked.map_or(Ok(git_dir), Err))
}

/// Whether any of `paths` differs from HEAD, staged, unstaged or untracked.
fn differs_from_head(root: &Path, paths: &[&str]) -> Result<bool> {
    let status = [
        "status",
        "--porcelain=v1",
        "-z",
        "--untracked-files=all",
        "--",
    ];
    Ok(!listed(root, "git status", &status, paths)?.is_empty())
}

/// The paths among `paths` git does not track (nor ignore), relative to `root` as given.
fn untracked(root: &Path, paths: &[&str]) -> Result<Vec<String>> {
    let others = ["ls-files", "-z", "--others", "--exclude-standard", "--"];
    let listing = listed(root, "git ls-files", &others, paths)?;
    let listing = String::from_utf8(listing).context("git listed a non-UTF-8 path")?;
    Ok(listing
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .collect())
}

/// What `step` (`args` over `paths`) prints, once it succeeded.
fn listed(root: &Path, step: &str, args: &[&str], paths: &[&str]) -> Result<Vec<u8>> {
    let output = run_literal(root, args, paths)?;
    ensure!(output.status.success(), "{}", refusal(step, &output));
    Ok(output.stdout)
}

/// `step`'s refusal as the reader sees it: its exit status and the last lines it printed (a
/// hook's own words included), cut short.
pub(super) fn refusal(step: &str, output: &Output) -> String {
    let printed = [&output.stderr, &output.stdout]
        .into_iter()
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        .collect::<Vec<_>>()
        .join("\n");
    let lines: Vec<&str> = printed
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let tail = lines[lines.len().saturating_sub(REFUSAL_LINES)..].join(" ");
    let status = output.status.code().map_or_else(
        || "was stopped by a signal".to_owned(),
        |code| format!("exited with {code}"),
    );
    let mut reason = format!("{step} {status}");
    if !tail.is_empty() {
        reason.push_str(": ");
        reason.push_str(&tail);
    }
    if reason.len() > REFUSAL_BYTES {
        let cut = (0..=REFUSAL_BYTES)
            .rev()
            .find(|index| reason.is_char_boundary(*index))
            .unwrap_or(0);
        reason.truncate(cut);
        reason.push('…');
    }
    reason
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lock_left_behind_by_a_timeout_is_named() {
        let dir = tempfile::tempdir().unwrap();
        let git_dir = dir.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        let error: anyhow::Error =
            ProcessTimeoutError::new("git commit", Duration::from_secs(600)).into();
        let reason = timed_out(dir.path(), &git_dir, &error);
        assert_eq!(reason, "git commit timed out after 600s and was stopped");
        std::fs::write(git_dir.join("index.lock"), "").unwrap();
        let reason = timed_out(dir.path(), &git_dir, &error);
        assert_eq!(
            reason,
            "git commit timed out after 600s and was stopped; .git/index.lock remains, so git \
             refuses other commits until it is removed"
        );
    }
}
