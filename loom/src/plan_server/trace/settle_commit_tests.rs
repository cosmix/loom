//! `settle_commit::record` over throwaway repositories: what a settle commits, what it leaves
//! alone, when it does not commit at all, and how it treats hooks it knows nothing about.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tempfile::TempDir;

use super::route_tests::{git, put};
use super::settle_commit::{record, Limits, Recorded, SettleCommit};

pub(super) const PLAN: &str = "doc/plans/demo/PLAN-first.md";
const BRIEF: &str = "doc/plans/demo/briefs/first/common.md";
pub(super) const NEW_BRIEF: &str = "doc/plans/demo/briefs/first/alpha/w1.md";
const STAGED: &str = "doc/product/demo/staged.md";
const LOOSE: &str = "doc/product/demo/loose.md";
/// A file a commit hook writes and adds beside the settled paths.
const GENERATED: &str = "doc/generated.md";

/// A repository whose own configuration names the identity, turns signing off and points hooks
/// at `.git/hooks`, so the user's global configuration cannot change what the tests see.
pub(super) fn configure(root: &Path) {
    for (key, value) in [
        ("user.name", "Test"),
        ("user.email", "test@example.com"),
        ("commit.gpgsign", "false"),
        ("core.hooksPath", ".git/hooks"),
    ] {
        git(root, &["config", key, value]);
    }
}

/// An executable hook `name` in `root` (configured by [`configure`]) running `script`.
pub(super) fn hook(root: &Path, name: &str, script: &str) {
    let rel = format!(".git/hooks/{name}");
    put(root, &rel, &format!("#!/bin/sh\n{script}"));
    let path = root.join(rel);
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
    std::fs::set_permissions(&path, permissions).unwrap();
}

/// An executable pre-commit hook in `root` running `script`.
pub(super) fn pre_commit(root: &Path, script: &str) {
    hook(root, "pre-commit", script);
}

/// A pre-commit hook that prints `words` and refuses.
pub(super) fn refusing_hook(root: &Path, words: &str) {
    pre_commit(root, &format!("echo '{words}' >&2\nexit 1\n"));
}

/// A committed plan with one brief, an unrelated staged change and an unrelated modified file.
pub(super) fn repo() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for rel in [PLAN, BRIEF, STAGED, LOOSE] {
        put(root, rel, "# Base\n");
    }
    git(root, &["init", "-q"]);
    configure(root);
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", "base"]);
    put(root, STAGED, "# Staged\n");
    git(root, &["add", STAGED]);
    put(root, LOOSE, "# Loose\n");
    dir
}

pub(super) fn plan_commit<'a>(briefs: &'a [PathBuf], clear: bool) -> SettleCommit<'a> {
    SettleCommit {
        feature: "demo",
        doc: Path::new(PLAN),
        briefs,
        clear,
    }
}

fn commit_of(recorded: Recorded) -> String {
    match recorded {
        Recorded::Committed {
            commit: Some(commit),
            note: None,
        } => commit,
        other => panic!("expected a clean commit, got {other:?}"),
    }
}

pub(super) fn refusal_of(recorded: Recorded) -> String {
    match recorded {
        Recorded::NotCommitted(reason) => reason,
        other => panic!("expected no commit, got {other:?}"),
    }
}

pub(super) fn head(root: &Path) -> String {
    git(root, &["rev-parse", "HEAD"])
}

fn committed_files(root: &Path) -> String {
    git(root, &["show", "--name-only", "--format=", "HEAD"])
}

/// The unrelated changes are as `repo` left them: one staged, one only modified.
pub(super) fn assert_untouched(root: &Path) {
    assert_eq!(git(root, &["diff", "--cached", "--name-only"]), STAGED);
    assert_eq!(git(root, &["diff", "--name-only"]), LOOSE);
}

/// After a commit that did not happen: the index is as `repo` left it, the written plan beside
/// the unrelated modified file.
pub(super) fn assert_left_staged(root: &Path) {
    assert_eq!(git(root, &["diff", "--cached", "--name-only"]), STAGED);
    assert_eq!(
        git(root, &["diff", "--name-only"]),
        format!("{PLAN}\n{LOOSE}")
    );
}

#[test]
fn settle_commits_only_its_paths_with_the_exact_message() {
    let dir = repo();
    let root = dir.path();
    put(root, PLAN, "# Base\n\nSettled: body=abc\n");
    let commit = commit_of(record(root, &plan_commit(&[], false), Limits::STANDARD));
    assert_eq!(commit, head(root));
    let message = git(root, &["log", "-1", "--format=%B"]);
    assert_eq!(message, "docs(demo): settle plans/demo/PLAN-first.md");
    assert_eq!(git(root, &["log", "-1", "--format=%(trailers)"]), "");
    assert_eq!(
        git(root, &["log", "-1", "--format=%an <%ae>"]),
        "Test <test@example.com>"
    );
    assert_eq!(committed_files(root), PLAN);
    assert_untouched(root);
}

#[test]
fn a_plan_settle_commits_its_briefs_new_and_changed() {
    let dir = repo();
    let root = dir.path();
    put(root, PLAN, "# Base\n\nSettled: body=abc\n");
    put(root, BRIEF, "# Brief, edited\n");
    put(root, NEW_BRIEF, "# New brief\n");
    let briefs = [PathBuf::from(NEW_BRIEF), PathBuf::from(BRIEF)];
    commit_of(record(root, &plan_commit(&briefs, false), Limits::STANDARD));
    let mut files: Vec<String> = committed_files(root).lines().map(str::to_owned).collect();
    files.sort();
    assert_eq!(files, [PLAN, NEW_BRIEF, BRIEF]);
    assert_eq!(git(root, &["status", "--porcelain", "--", "doc/plans"]), "");
    assert_untouched(root);
}

#[test]
fn unsettle_commits_the_document_alone() {
    let dir = repo();
    let root = dir.path();
    put(root, PLAN, "# Base, unsettled\n");
    put(root, BRIEF, "# Brief, edited\n");
    let briefs = [PathBuf::from(BRIEF)];
    commit_of(record(root, &plan_commit(&briefs, true), Limits::STANDARD));
    let message = git(root, &["log", "-1", "--format=%B"]);
    assert_eq!(message, "docs(demo): unsettle plans/demo/PLAN-first.md");
    assert_eq!(committed_files(root), PLAN);
    assert_eq!(git(root, &["diff", "--name-only", "--", BRIEF]), BRIEF);
}

#[test]
fn nothing_to_commit_makes_no_commit() {
    let dir = repo();
    let root = dir.path();
    let before = head(root);
    let recorded = record(root, &plan_commit(&[], false), Limits::STANDARD);
    assert_eq!(recorded, Recorded::Unchanged);
    assert_eq!(head(root), before);
    assert_untouched(root);
}

#[test]
fn a_refusing_hook_reports_its_words_and_unstages_the_new_brief() {
    let dir = repo();
    let root = dir.path();
    refusing_hook(root, "docs are frozen");
    put(root, PLAN, "# Base\n\nSettled: body=abc\n");
    put(root, NEW_BRIEF, "# New brief\n");
    let before = head(root);
    let briefs = [PathBuf::from(NEW_BRIEF)];
    let reason = refusal_of(record(root, &plan_commit(&briefs, false), Limits::STANDARD));
    assert!(reason.starts_with("git commit exited with 1"), "{reason}");
    assert!(reason.contains("docs are frozen"), "{reason}");
    assert_eq!(head(root), before);
    assert_eq!(
        std::fs::read_to_string(root.join(PLAN)).unwrap(),
        "# Base\n\nSettled: body=abc\n"
    );
    assert_eq!(git(root, &["ls-files", "--", NEW_BRIEF]), "", "unstaged");
    assert!(root.join(NEW_BRIEF).is_file());
    assert_left_staged(root);
}

#[test]
fn a_hook_that_rewrites_and_adds_leaves_the_index_at_head() {
    let dir = repo();
    let root = dir.path();
    pre_commit(
        root,
        &format!("printf 'Formatted.\\n' >> {PLAN}\ngit add {PLAN}\n"),
    );
    put(root, PLAN, "# Base\n\nSettled: body=abc\n");
    commit_of(record(root, &plan_commit(&[], false), Limits::STANDARD));
    let committed = git(root, &["show", &format!("HEAD:{PLAN}")]);
    assert!(committed.ends_with("Formatted."), "{committed}");
    assert_eq!(git(root, &["diff", "--name-only", "HEAD", "--", PLAN]), "");
    assert_eq!(
        git(root, &["diff", "--cached", "--name-only", "--", PLAN]),
        ""
    );
    assert_untouched(root);
}

/// Under `--only` the hook's `git add` reaches git's temporary index alone: without the reset the
/// real index would lack the file the commit holds, a staged deletion of it.
#[test]
fn a_file_a_hook_generates_and_adds_is_committed_and_left_clean() {
    let dir = repo();
    let root = dir.path();
    pre_commit(
        root,
        &format!("printf 'Generated.\\n' > {GENERATED}\ngit add {GENERATED}\n"),
    );
    put(root, PLAN, "# Base\n\nSettled: body=abc\n");
    commit_of(record(root, &plan_commit(&[], false), Limits::STANDARD));
    assert_eq!(committed_files(root), format!("{GENERATED}\n{PLAN}"));
    assert_eq!(git(root, &["status", "--porcelain", "--", GENERATED]), "");
    assert_eq!(git(root, &["status", "--porcelain", "--", PLAN]), "");
    assert_untouched(root);
}

#[test]
fn a_staged_file_a_hook_also_commits_keeps_its_staged_entry_and_says_so() {
    let dir = repo();
    let root = dir.path();
    pre_commit(
        root,
        &format!("printf 'Hooked.\\n' > {STAGED}\ngit add {STAGED}\n"),
    );
    put(root, PLAN, "# Base\n\nSettled: body=abc\n");
    let note = match record(root, &plan_commit(&[], false), Limits::STANDARD) {
        Recorded::Committed {
            commit: Some(commit),
            note: Some(note),
        } if commit == head(root) => note,
        other => panic!("expected a commit with a note, got {other:?}"),
    };
    assert_eq!(
        note,
        format!(
            "A commit hook also committed {STAGED}, whose changes staged before the commit stay \
             staged and differ from it; check them before the next commit."
        )
    );
    assert_eq!(git(root, &["show", &format!("HEAD:{STAGED}")]), "Hooked.");
    assert_eq!(git(root, &["show", &format!(":{STAGED}")]), "# Staged");
    assert_eq!(git(root, &["diff", "--cached", "--name-only"]), STAGED);
    assert_eq!(git(root, &["status", "--porcelain", "--", PLAN]), "");
}

/// A project below the top of its repository: the settled paths are the project's, and the hook,
/// run from the top, adds a file outside it.
#[test]
fn a_project_below_the_top_of_its_repository_leaves_the_index_at_head() {
    let dir = tempfile::tempdir().unwrap();
    let top = dir.path();
    let root = top.join("project");
    put(&root, PLAN, "# Base\n");
    git(top, &["init", "-q"]);
    configure(top);
    git(top, &["add", "-A"]);
    git(top, &["commit", "-q", "-m", "base"]);
    let script = format!(
        "printf 'Formatted.\\n' >> project/{PLAN}\nprintf 'Top.\\n' > top.md\n\
         git add project/{PLAN} top.md\n"
    );
    pre_commit(top, &script);
    put(&root, PLAN, "# Base\n\nSettled: body=abc\n");
    commit_of(record(&root, &plan_commit(&[], false), Limits::STANDARD));
    let committed = git(top, &["show", &format!("HEAD:project/{PLAN}")]);
    assert!(committed.ends_with("Formatted."), "{committed}");
    assert_eq!(git(top, &["status", "--porcelain"]), "");
}

#[test]
fn the_hook_does_not_see_literal_pathspecs() {
    let dir = repo();
    let root = dir.path();
    pre_commit(
        root,
        "echo \"literal=${GIT_LITERAL_PATHSPECS:-unset}\" > .git/hook-env\n",
    );
    put(root, PLAN, "# Base\n\nSettled: body=abc\n");
    commit_of(record(root, &plan_commit(&[], false), Limits::STANDARD));
    let seen = std::fs::read_to_string(root.join(".git/hook-env")).unwrap();
    assert_eq!(seen, "literal=unset\n");
}

#[test]
fn a_hook_past_the_deadline_is_terminated_and_its_lock_released() {
    let dir = repo();
    let root = dir.path();
    pre_commit(root, "sleep 30\n");
    put(root, PLAN, "# Base\n\nSettled: body=abc\n");
    put(root, NEW_BRIEF, "# New brief\n");
    let before = head(root);
    let limits = Limits {
        timeout: Duration::from_secs(2),
        grace: Duration::from_secs(5),
    };
    let briefs = [PathBuf::from(NEW_BRIEF)];
    let reason = refusal_of(record(root, &plan_commit(&briefs, false), limits));
    assert!(reason.starts_with("git commit timed out after"), "{reason}");
    assert!(reason.contains("was stopped"), "{reason}");
    // `SIGTERM` lets git remove its lock; a `SIGKILL` would have left it.
    assert!(!reason.contains("index.lock"), "{reason}");
    assert!(!root.join(".git/index.lock").exists());
    assert_eq!(head(root), before);
    assert_eq!(git(root, &["ls-files", "--", NEW_BRIEF]), "", "unstaged");
    assert_left_staged(root);
}

#[test]
fn a_merge_in_progress_skips_the_commit() {
    let dir = repo();
    let root = dir.path();
    put(root, PLAN, "# Base\n\nSettled: body=abc\n");
    let before = head(root);
    put(root, ".git/MERGE_HEAD", &format!("{before}\n"));
    let recorded = record(root, &plan_commit(&[], false), Limits::STANDARD);
    assert_eq!(
        recorded,
        Recorded::NotCommitted(
            "a merge is in progress; commit the document once it is done".to_owned()
        )
    );
    assert_eq!(head(root), before);
}

#[test]
fn a_project_outside_git_is_not_committed() {
    let dir = tempfile::tempdir().unwrap();
    put(dir.path(), PLAN, "# Plan\n");
    let recorded = record(dir.path(), &plan_commit(&[], false), Limits::STANDARD);
    assert_eq!(
        recorded,
        Recorded::NotCommitted("this project is not in a git repository".to_owned())
    );
}
