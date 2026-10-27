//! Tests for files::diff: the uncommitted diff against `HEAD` and the turn diff from pins.

use std::path::Path;
use std::process::Command;

use super::doc::{context, write_files};
use crate::plan_server::files::diff::{head_texts, turn_texts};
use crate::plan_server::files::doc::text_store;
use crate::plan_server::files::texts::TurnId;

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(args)
        .current_dir(dir)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .status()
        .expect("run git");
    assert!(status.success(), "git {args:?} failed");
}

fn commit_all(dir: &Path) {
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", "init"]);
}

#[test]
fn head_diff_of_a_committed_then_modified_file() {
    let committed = "# A\n\nCommitted.\n";
    let dir = write_files(&[("doc/product/a.md", committed)]);
    commit_all(dir.path());
    let modified = "# A\n\nModified.\n";
    std::fs::write(dir.path().join("doc/product/a.md"), modified).expect("modify");
    let ctx = context(&dir);
    let diff = head_texts(&ctx, Some("product/a.md")).expect("head diff");
    assert_eq!(diff.path, "doc/product/a.md");
    assert_eq!(diff.before.as_deref(), Some(committed));
    assert_eq!(diff.after.as_deref(), Some(modified));
}

#[test]
fn head_diff_of_an_untracked_file_has_no_before() {
    let dir = write_files(&[]);
    commit_all(dir.path());
    std::fs::write(dir.path().join("doc/product/new.md"), "# New\n").expect("write");
    let ctx = context(&dir);
    let diff = head_texts(&ctx, Some("product/new.md")).expect("head diff");
    assert_eq!(diff.before, None);
    assert_eq!(diff.after.as_deref(), Some("# New\n"));
}

#[test]
fn head_diff_refuses_bad_and_undiscovered_paths() {
    let dir = write_files(&[(".env", "SECRET=hunter2\n")]);
    std::fs::create_dir_all(dir.path().join("doc/notes")).expect("mkdir");
    std::os::unix::fs::symlink("../../.env", dir.path().join("doc/notes/out.md")).expect("symlink");
    commit_all(dir.path());
    let ctx = context(&dir);
    let cases = [
        ("../.env", 400),
        ("product/../../.env", 400),
        ("/etc/passwd", 400),
        ("-p", 404),
        ("notes/out.md", 404),
        ("product/missing.md", 404),
    ];
    for (raw, status) in cases {
        let refusal = head_texts(&ctx, Some(raw)).expect_err(raw);
        assert_eq!(refusal.status, status, "{raw}");
        assert!(!String::from_utf8_lossy(&refusal.body).contains("hunter2"));
    }
    assert_eq!(head_texts(&ctx, None).expect_err("missing").status, 400);
}

#[test]
fn turn_diff_from_pins() {
    let dir = write_files(&[
        ("doc/product/edited.md", "# E\n\nBefore.\n"),
        ("doc/product/deleted.md", "# D\n"),
        ("doc/product/same.md", "# S\n"),
    ]);
    let ctx = context(&dir);
    let store = text_store(&ctx).expect("store");
    let turn = TurnId("session/t1".to_owned());
    store.begin_turn(&turn);
    let root = dir.path();
    std::fs::write(root.join("doc/product/edited.md"), "# E\n\nAfter.\n").expect("edit");
    std::fs::write(root.join("doc/product/created.md"), "# C\n").expect("create");
    std::fs::remove_file(root.join("doc/product/deleted.md")).expect("delete");
    let changes = {
        let project = ctx.project();
        store.end_turn(&turn, root, &project)
    };
    assert_eq!(changes.len(), 3, "{changes:?}");
    let diff = |raw| turn_texts(&ctx, Some("session/t1"), Some(raw));
    let edited = diff("product/edited.md").expect("edited");
    assert_eq!(edited.path, "doc/product/edited.md");
    assert_eq!(edited.before.as_deref(), Some("# E\n\nBefore.\n"));
    assert_eq!(edited.after.as_deref(), Some("# E\n\nAfter.\n"));
    let created = diff("product/created.md").expect("created");
    assert_eq!(
        (created.before, created.after.as_deref()),
        (None, Some("# C\n"))
    );
    let deleted = diff("product/deleted.md").expect("deleted");
    assert_eq!(
        (deleted.before.as_deref(), deleted.after),
        (Some("# D\n"), None)
    );
    assert_eq!(diff("product/same.md").expect_err("unchanged").status, 404);
    assert_eq!(diff("../x.md").expect_err("syntax").status, 400);
    let unknown = turn_texts(&ctx, Some("session/nope"), Some("product/edited.md"));
    assert_eq!(unknown.expect_err("unknown turn").status, 404);
    let no_turn = turn_texts(&ctx, None, Some("product/edited.md"));
    assert_eq!(no_turn.expect_err("missing turn").status, 400);
}
