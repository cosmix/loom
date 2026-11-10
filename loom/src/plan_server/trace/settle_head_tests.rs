//! `settle_commit::record` when git's exit status does not say what happened: a `post-commit`
//! hook that runs past the deadline, prints past the output limit or commits itself, a
//! `commit-msg` hook that rewrites the message, an error before any commit, a commit that is not
//! the settle's, an unborn branch, a log encoding other than UTF-8.

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::route_tests::{git, put};
use super::settle_commit::{record, Limits, Recorded, SettleCommit};
use super::settle_commit_tests::{
    assert_left_staged, assert_untouched, configure, head, hook, plan_commit, pre_commit,
    refusal_of, repo, NEW_BRIEF, PLAN,
};

/// A deadline a test's hook outlives.
const SHORT: Limits = Limits {
    timeout: Duration::from_secs(2),
    grace: Duration::from_secs(5),
};

/// A hook script that prints past the 16 MiB output limit.
const FLOOD: &str = "head -c 17000000 /dev/zero >&2\n";

/// A made commit's id and note.
fn made(recorded: Recorded) -> (Option<String>, Option<String>) {
    match recorded {
        Recorded::Committed { commit, note } => (commit, note),
        other => panic!("expected a commit, got {other:?}"),
    }
}

/// Write the plan's line and a new brief, and record the settle of both under `limits`.
fn settle_with_new_brief(root: &Path, limits: Limits) -> Recorded {
    put(root, PLAN, "# Base\n\nSettled: body=abc\n");
    put(root, NEW_BRIEF, "# New brief\n");
    let briefs = [PathBuf::from(NEW_BRIEF)];
    record(root, &plan_commit(&briefs, false), limits)
}

/// The commit holds the plan and the new brief, and nothing is staged for deletion: the index
/// holds HEAD's text for both, the unrelated changes as they were.
fn assert_committed_with_its_brief(root: &Path) {
    let files = git(root, &["show", "--name-only", "--format=", "HEAD"]);
    assert_eq!(files.lines().collect::<Vec<_>>(), [PLAN, NEW_BRIEF]);
    assert_eq!(git(root, &["ls-files", "--", NEW_BRIEF]), NEW_BRIEF);
    assert_untouched(root);
}

#[test]
fn a_post_commit_hook_past_the_deadline_still_reports_the_commit() {
    let dir = repo();
    let root = dir.path();
    hook(root, "post-commit", "sleep 30\n");
    let before = head(root);
    let (commit, note) = made(settle_with_new_brief(root, SHORT));
    assert_ne!(head(root), before);
    assert_eq!(commit, Some(head(root)));
    let note = note.expect("a note");
    assert!(
        note.starts_with("The commit was made, but git commit timed out after"),
        "{note}"
    );
    assert_committed_with_its_brief(root);
}

#[test]
fn a_post_commit_hook_past_the_output_limit_still_reports_the_commit() {
    let dir = repo();
    let root = dir.path();
    hook(root, "post-commit", FLOOD);
    let (commit, note) = made(settle_with_new_brief(root, Limits::STANDARD));
    assert_eq!(commit, Some(head(root)));
    let note = note.expect("a note");
    assert!(note.contains("output limit"), "{note}");
    assert_committed_with_its_brief(root);
}

#[test]
fn a_failure_before_any_commit_unstages_the_new_brief() {
    let dir = repo();
    let root = dir.path();
    pre_commit(root, &format!("{FLOOD}exit 1\n"));
    let before = head(root);
    let reason = refusal_of(settle_with_new_brief(root, Limits::STANDARD));
    assert!(reason.starts_with("git commit failed: "), "{reason}");
    assert!(reason.contains("output limit"), "{reason}");
    assert_eq!(head(root), before);
    assert_eq!(git(root, &["ls-files", "--", NEW_BRIEF]), "", "unstaged");
    assert_left_staged(root);
}

#[test]
fn a_post_commit_hook_that_commits_leaves_the_settle_its_own_commit() {
    let dir = repo();
    let root = dir.path();
    // A post-commit hook after `commit --only` inherits an index file git has already removed.
    let script = "[ -e .git/hooked ] && exit 0\ntouch .git/hooked\nunset GIT_INDEX_FILE\n\
                  git commit -q --allow-empty -m 'hook commit'\n";
    hook(root, "post-commit", script);
    let (commit, note) = made(settle_with_new_brief(root, Limits::STANDARD));
    assert_eq!(commit, Some(git(root, &["rev-parse", "HEAD^"])));
    assert_eq!(note, None);
    let subjects = git(root, &["log", "--format=%s"]);
    assert_eq!(
        subjects.lines().collect::<Vec<_>>(),
        [
            "hook commit",
            "docs(demo): settle plans/demo/PLAN-first.md",
            "base"
        ]
    );
    assert_eq!(
        git(root, &["diff", "--cached", "--name-only", "--", PLAN]),
        ""
    );
}

#[test]
fn a_rewritten_message_is_committed_without_an_id() {
    let dir = repo();
    let root = dir.path();
    hook(
        root,
        "commit-msg",
        "printf '[T-1] %s\\n' \"$(cat \"$1\")\" > \"$1.new\" && mv \"$1.new\" \"$1\"\n",
    );
    let (commit, note) = made(settle_with_new_brief(root, Limits::STANDARD));
    assert_eq!(commit, None);
    assert_eq!(
        note.as_deref(),
        Some(
            "The commit was made, but HEAD does not show it with the settle's message, so its \
             id is not known."
        )
    );
    let subject = git(root, &["log", "-1", "--format=%s"]);
    assert_eq!(subject, "[T-1] docs(demo): settle plans/demo/PLAN-first.md");
    assert_committed_with_its_brief(root);
}

#[test]
fn a_settle_on_an_unborn_branch_reports_the_root_commit() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, PLAN, "# Plan\n\nSettled: body=abc\n");
    git(root, &["init", "-q"]);
    configure(root);
    let (commit, note) = made(record(root, &plan_commit(&[], false), Limits::STANDARD));
    assert_eq!(commit, Some(head(root)));
    assert_eq!(note, None);
    assert_eq!(git(root, &["rev-list", "--parents", "HEAD"]), head(root));
}

/// A hook that moves HEAD to a commit of its own and refuses: the settle's commit never happened,
/// and the brief it added, which no commit holds, is unstaged.
#[test]
fn a_refusal_after_head_moved_elsewhere_unstages_the_new_brief() {
    let dir = repo();
    let root = dir.path();
    let script = "tree=$(git rev-parse 'HEAD^{tree}')\n\
                  next=$(git commit-tree -p HEAD -m other \"$tree\")\n\
                  git update-ref HEAD \"$next\"\nexit 1\n";
    pre_commit(root, script);
    let reason = refusal_of(settle_with_new_brief(root, Limits::STANDARD));
    assert!(reason.starts_with("git commit exited with 1"), "{reason}");
    assert!(
        reason.ends_with(
            "HEAD moved to a commit that is not the settle's, so the index was not reset to it"
        ),
        "{reason}"
    );
    assert_eq!(git(root, &["log", "-1", "--format=%s"]), "other");
    assert_eq!(git(root, &["ls-files", "--", NEW_BRIEF]), "", "unstaged");
    assert!(root.join(NEW_BRIEF).is_file());
    assert_left_staged(root);
}

#[test]
fn a_settle_finds_its_commit_whatever_the_log_encoding() {
    let dir = repo();
    let root = dir.path();
    git(root, &["config", "i18n.logOutputEncoding", "ISO-8859-1"]);
    let doc = "doc/product/demo/café.md";
    put(root, doc, "# Café\n\nSettled: body=abc\n");
    let commit = SettleCommit {
        feature: "demo",
        doc: Path::new(doc),
        briefs: &[],
        clear: false,
    };
    let (commit, note) = made(record(root, &commit, Limits::STANDARD));
    assert_eq!(commit, Some(head(root)));
    assert_eq!(note, None);
}
