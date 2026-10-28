//! Fixtures for the review-loop contracts: git repositories in a TempDir with commits at fixed
//! dates, plan and run-review documents, and suggestion senders.
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, UNIX_EPOCH};

use loom::plan_server::review::suggest::{BrowserRef, SuggestedPassage};
use tempfile::TempDir;

/// The product section the built-marker fixtures change, repo-relative.
pub const SECTION_02: &str = "doc/product/section_02.md";
/// The build date every fixture review carries.
pub const BUILT: &str = "2026-10-20 14:05 UTC";
/// Commit dates on either side of [`BUILT`].
pub const BEFORE_BUILD: &str = "2026-10-19T10:00:00Z";
pub const AFTER_BUILD: &str = "2026-10-21T10:00:00Z";

/// A git repository under `<tmp>/repo` on branch `main`; the TempDir lives as long as this.
pub struct Repo {
    _temp: TempDir,
    pub root: PathBuf,
}

pub fn repo() -> Repo {
    let temp = tempfile::Builder::new()
        .prefix("loom-review-contracts-")
        .tempdir()
        .expect("create scratch directory");
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

/// Runs git in `root` isolated from the user's and system configuration, with author and
/// committer dates set to `date` when given; panics on failure and returns trimmed stdout.
pub fn git(root: &Path, args: &[&str], date: Option<&str>) -> String {
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

pub fn put(root: &Path, rel: &str, content: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().expect("fixture path has a parent")).expect("mkdir");
    fs::write(path, content).expect("write fixture file");
}

/// Commits everything in the tree at `date` and returns the new HEAD's 40-hex id.
pub fn commit(root: &Path, date: &str, message: &str) -> String {
    git(root, &["add", "-A"], None);
    git(root, &["commit", "-q", "-m", message], Some(date));
    git(root, &["rev-parse", "HEAD"], None)
}

/// Sets the file's modification time to the RFC 3339 instant `at`.
pub fn set_mtime(root: &Path, rel: &str, at: &str) {
    let at = chrono::DateTime::parse_from_rfc3339(at).expect("RFC 3339 instant");
    let secs = u64::try_from(at.timestamp()).expect("instant after the epoch");
    let file = fs::File::options()
        .write(true)
        .open(root.join(rel))
        .expect("open fixture file");
    file.set_modified(UNIX_EPOCH + Duration::from_secs(secs))
        .expect("set mtime");
}

/// A plan document implementing section 2.
pub fn plan_doc(slug: &str) -> String {
    format!("# Plan: {slug}\n\nImplements: section_02\n\nDoes {slug}.\n")
}

/// A run review in the shape `fs::plan_review` writes, for `plan_id` built at `generated`.
pub fn review_doc(plan_id: &str, generated: &str) -> String {
    format!(
        "# Code Review: {plan_id}\n\n**Plan:** {plan_id} | **Generated:** {generated}\n\n\
         ## Summary\n\nBuilt the section.\n"
    )
}

/// Section 2 committed before [`BUILT`], then changed and committed after it; returns the
/// first commit. `files` are committed with the first version.
pub fn section_changed_after_build(files: &[(&str, String)]) -> (Repo, String) {
    let repo = repo();
    let root = &repo.root;
    put(root, SECTION_02, "# Section 2\n\nFirst text.\n");
    for (rel, text) in files {
        put(root, rel, text);
    }
    let base = commit(root, BEFORE_BUILD, "first section text");
    put(root, SECTION_02, "# Section 2\n\nSecond text.\n");
    commit(root, AFTER_BUILD, "second section text");
    (repo, base)
}

pub fn browser(id: &str, public_id: &str, name: &str) -> BrowserRef {
    BrowserRef {
        id: id.to_owned(),
        public_id: public_id.to_owned(),
        name: name.to_owned(),
    }
}

/// A read-only browser and the authoring holder.
pub fn reader_and_holder() -> (BrowserRef, BrowserRef) {
    (
        browser("s-secret", "p1", "Sam"),
        browser("h-secret", "p2", "Dana"),
    )
}

pub fn passage() -> SuggestedPassage {
    SuggestedPassage {
        path: "doc/product/section_01.md".to_owned(),
        anchor: "goals".to_owned(),
        text: "The planner keeps one author at a time.".to_owned(),
        lines: Some((3, 4)),
        stage: None,
        summary_sentence: None,
    }
}
