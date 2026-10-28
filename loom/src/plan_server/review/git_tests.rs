//! Tests for compare and built against git fixtures.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::{TimeZone, Utc};
use tempfile::TempDir;

use super::built::{built_notes, parse_generated, BuiltNote};
use super::compare::{changed_since, commit_exists, text_at, ChangedDoc, ChangedDocChange};
use crate::planning::project::Project;

const SECTION: &str = "doc/product/section_02.md";
const SECTION_ARG: &str = "product/section_02.md";
const BEFORE: &str = "1999-06-01T10:00:00Z";
const BUILT: &str = "2000-01-01 00:00 UTC";
const LATER_BUILT: &str = "2000-01-02 00:00 UTC";
const AFTER: &str = "2000-01-03T10:00:00Z";

/// A repository under `<tmp>/repo` isolated from user and system git config.
struct Repo {
    _temp: TempDir,
    root: PathBuf,
}

fn git(root: &Path, args: &[&str], date: Option<&str>) -> String {
    let mut cmd = Command::new("git");
    cmd.args(args)
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null");
    if let Some(date) = date {
        cmd.env("GIT_AUTHOR_DATE", date)
            .env("GIT_COMMITTER_DATE", date);
    }
    let out = cmd.output().expect("run git");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout)
        .expect("git stdout is UTF-8")
        .trim()
        .to_owned()
}

fn repo() -> Repo {
    let temp = TempDir::new().expect("create scratch directory");
    let root = temp.path().join("repo");
    fs::create_dir_all(&root).expect("create repo dir");
    git(&root, &["init", "-q", "-b", "main"], None);
    git(&root, &["config", "user.name", "Review Fixture"], None);
    git(
        &root,
        &["config", "user.email", "fixture@example.invalid"],
        None,
    );
    git(&root, &["config", "commit.gpgsign", "false"], None);
    Repo { _temp: temp, root }
}

fn put(root: &Path, rel: &str, content: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().expect("fixture path has a parent")).expect("mkdir");
    fs::write(path, content).expect("write fixture file");
}

fn commit(root: &Path, date: &str) -> String {
    git(root, &["add", "-A"], None);
    git(root, &["commit", "-q", "-m", "fixture"], Some(date));
    git(root, &["rev-parse", "HEAD"], None)
}

fn doc(path: &str, change: ChangedDocChange) -> ChangedDoc {
    ChangedDoc {
        path: path.to_owned(),
        change,
    }
}

fn plan_doc(slug: &str) -> String {
    format!("# Plan: {slug}\n\nImplements: section_02\n\nDoes {slug}.\n")
}

fn review_doc(plan_id: &str, generated: &str) -> String {
    format!("# Code Review: {plan_id}\n\n**Plan:** {plan_id} | **Generated:** {generated}\n")
}

/// Section 2 and `files` committed at [`BEFORE`] (returned), then the section changed and
/// committed at [`AFTER`] when `change_later` holds.
fn built_fixture(files: &[(&str, String)], change_later: bool) -> (Repo, String) {
    let repo = repo();
    put(&repo.root, SECTION, "# Section 2\n\nFirst text.\n");
    for (rel, text) in files {
        put(&repo.root, rel, text);
    }
    let base = commit(&repo.root, BEFORE);
    if change_later {
        put(&repo.root, SECTION, "# Section 2\n\nSecond text.\n");
        commit(&repo.root, AFTER);
    }
    (repo, base)
}

fn notes_of(repo: &Repo) -> Vec<BuiltNote> {
    let project = Project::load(&repo.root).expect("load fixture project");
    built_notes(&project, SECTION_ARG).expect("built notes")
}

fn changed_after(slug: &str, built: &str, base: &str) -> BuiltNote {
    BuiltNote::ChangedAfter {
        slug: slug.to_owned(),
        built: built.to_owned(),
        base_commit: Some(base.to_owned()),
    }
}

#[test]
fn rename_shows_removed_plus_added() {
    let repo = repo();
    put(
        &repo.root,
        "doc/a.md",
        "# A\n\nSame text for rename detection.\n",
    );
    let head = commit(&repo.root, BEFORE);
    git(&repo.root, &["mv", "doc/a.md", "doc/b.md"], None);
    let docs = changed_since(&repo.root, &head).expect("changed since");
    assert_eq!(
        docs,
        vec![
            doc("doc/a.md", ChangedDocChange::Removed),
            doc("doc/b.md", ChangedDocChange::Added),
        ]
    );
}

#[test]
fn untracked_document_is_added() {
    let repo = repo();
    put(&repo.root, "doc/a.md", "# A\n");
    let head = commit(&repo.root, BEFORE);
    put(&repo.root, "doc/a.md", "# A\n\nEdited.\n");
    put(&repo.root, "doc/new.md", "# New\n");
    let docs = changed_since(&repo.root, &head).expect("changed since");
    assert_eq!(
        docs,
        vec![
            doc("doc/a.md", ChangedDocChange::Changed),
            doc("doc/new.md", ChangedDocChange::Added),
        ]
    );
}

#[test]
fn summary_store_paths_dropped() {
    let repo = repo();
    put(&repo.root, "doc/a.md", "# A\n");
    put(&repo.root, "doc/loom/summaries/x.json", "{}\n");
    let head = commit(&repo.root, BEFORE);
    put(&repo.root, "doc/a.md", "# A\n\nEdited.\n");
    put(&repo.root, "doc/loom/summaries/x.json", "{\"a\":1}\n");
    put(&repo.root, "doc/loom/summaries/y.json", "{}\n");
    let docs = changed_since(&repo.root, &head).expect("changed since");
    assert_eq!(docs, vec![doc("doc/a.md", ChangedDocChange::Changed)]);
}

#[test]
fn non_hex_commit_refused_before_git_runs() {
    // No repository at this path: if git ran, the error would be a spawn or repo failure.
    let temp = TempDir::new().expect("create scratch directory");
    let missing = temp.path().join("no-repo");
    let upper = "A".repeat(40);
    for commit in ["main", "--output=x", upper.as_str(), "abc"] {
        let changed = changed_since(&missing, commit).expect_err("non-hex commit refused");
        assert!(
            changed.to_string().contains("40 lowercase hex"),
            "{changed:#}"
        );
        let text = text_at(&missing, commit, "a.md").expect_err("non-hex commit refused");
        assert!(text.to_string().contains("40 lowercase hex"), "{text:#}");
        assert!(!commit_exists(&missing, commit));
    }
}

#[test]
fn text_at_missing_path_is_none() {
    let repo = repo();
    put(&repo.root, "doc/a.md", "# A\n");
    let head = commit(&repo.root, BEFORE);
    put(&repo.root, "doc/later.md", "# Later\n");
    git(&repo.root, &["add", "doc/later.md"], None);
    assert_eq!(
        text_at(&repo.root, &head, "a.md").expect("text at"),
        Some("# A\n".to_owned())
    );
    assert_eq!(
        text_at(&repo.root, &head, "gone.md").expect("text at"),
        None
    );
    assert_eq!(
        text_at(&repo.root, &head, "later.md").expect("text at"),
        None,
        "a path on disk and in the index but not in the commit is None"
    );
}

#[test]
fn uncommitted_section_counts_as_changed_now() {
    let files = [
        ("doc/plans/DONE-PLAN-u.md", plan_doc("u")),
        ("doc/plans/REVIEW-PLAN-u.md", review_doc("PLAN-u", BUILT)),
    ];
    let (repo, base) = built_fixture(&files, false);
    assert!(notes_of(&repo).is_empty(), "committed before the build");
    put(&repo.root, SECTION, "# Section 2\n\nUncommitted text.\n");
    assert_eq!(
        notes_of(&repo),
        vec![changed_after("u", "2000-01-01T00:00:00Z", &base)]
    );
}

#[test]
fn done_plan_without_review_gives_nothing() {
    let files = [("doc/plans/DONE-PLAN-d.md", plan_doc("d"))];
    let (repo, _) = built_fixture(&files, true);
    assert!(notes_of(&repo).is_empty());
}

#[test]
fn section_changed_before_the_build_gives_nothing() {
    let files = [
        ("doc/plans/DONE-PLAN-e.md", plan_doc("e")),
        (
            "doc/plans/REVIEW-PLAN-e.md",
            review_doc("PLAN-e", "2001-01-01 00:00 UTC"),
        ),
    ];
    let (repo, _) = built_fixture(&files, true);
    assert!(notes_of(&repo).is_empty());
}

#[test]
fn retired_and_written_plans_give_nothing() {
    let files = [
        ("doc/plans/RETIRED-PLAN-r.md", plan_doc("r")),
        ("doc/plans/REVIEW-PLAN-r.md", review_doc("PLAN-r", BUILT)),
        ("doc/plans/PLAN-w.md", plan_doc("w")),
        ("doc/plans/REVIEW-PLAN-w.md", review_doc("PLAN-w", BUILT)),
    ];
    let (repo, _) = built_fixture(&files, true);
    assert!(notes_of(&repo).is_empty());
}

#[test]
fn first_generated_line_wins() {
    let review = "# Code Review: a\n\n**Plan:** PLAN-a | **Generated:** 2026-10-20 14:05 UTC\n\n\
                  Quoted: **Generated:** 2027-01-01 00:00 UTC\n";
    let expected = Utc.with_ymd_and_hms(2026, 10, 20, 14, 5, 0).single();
    assert_eq!(parse_generated(review), expected);
}

#[test]
fn commit_exists_false_for_an_unknown_commit() {
    let repo = repo();
    put(&repo.root, "doc/a.md", "# A\n");
    let head = commit(&repo.root, BEFORE);
    let tree = git(&repo.root, &["rev-parse", "HEAD^{tree}"], None);
    assert!(commit_exists(&repo.root, &head));
    assert!(!commit_exists(&repo.root, &"0".repeat(40)));
    assert!(!commit_exists(&repo.root, &tree), "a tree is not a commit");
}

#[test]
fn review_under_archive_is_found() {
    let files = [
        ("doc/plans/archive/PLAN-a.md", plan_doc("a")),
        (
            "doc/plans/archive/REVIEW-PLAN-a.md",
            review_doc("PLAN-a", BUILT),
        ),
    ];
    let (repo, base) = built_fixture(&files, true);
    assert_eq!(
        notes_of(&repo),
        vec![changed_after("a", "2000-01-01T00:00:00Z", &base)]
    );
}

#[test]
fn review_in_progress_is_found() {
    let files = [
        ("doc/plans/DONE-PLAN-c.md", plan_doc("c")),
        (
            "doc/plans/REVIEW-IN_PROGRESS-PLAN-c.md",
            review_doc("IN_PROGRESS-PLAN-c", BUILT),
        ),
    ];
    let (repo, base) = built_fixture(&files, true);
    assert_eq!(
        notes_of(&repo),
        vec![changed_after("c", "2000-01-01T00:00:00Z", &base)]
    );
}

#[test]
fn newest_of_two_reviews_wins() {
    let files = [
        ("doc/plans/DONE-PLAN-n.md", plan_doc("n")),
        ("doc/plans/REVIEW-PLAN-n.md", review_doc("PLAN-n", BUILT)),
        (
            "doc/plans/archive/REVIEW-DONE-PLAN-n.md",
            review_doc("DONE-PLAN-n", LATER_BUILT),
        ),
    ];
    let (repo, base) = built_fixture(&files, true);
    assert_eq!(
        notes_of(&repo),
        vec![changed_after("n", "2000-01-02T00:00:00Z", &base)]
    );
}
