//! Tests for files::diff: the uncommitted diff against `HEAD` and the turn diff from pins.

use std::path::Path;
use std::process::Command;

use super::doc::{context, write_files};
use crate::plan_server::context::PlanContext;
use crate::plan_server::files::diff::{head_texts, turn_texts};
use crate::plan_server::files::doc::{text_store, MAX_EDIT_BYTES};
use crate::plan_server::files::texts::{TurnId, MAX_TURNS};

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
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

/// End `turn` the way a caller does: against the context's project and the files on disk now.
fn end_turn(ctx: &PlanContext, turn: &TurnId) {
    let store = text_store(ctx).expect("store");
    let project = ctx.project();
    store.end_turn(turn, ctx.root(), &project);
}

#[test]
fn head_diff_refuses_a_side_over_one_mib() {
    let exact = "x".repeat(MAX_EDIT_BYTES);
    let big = "x".repeat(MAX_EDIT_BYTES + 1);
    let dir = write_files(&[
        ("doc/product/exact.md", exact.as_str()),
        ("doc/product/big.md", big.as_str()),
        ("doc/product/grows.md", "# G\n"),
    ]);
    commit_all(dir.path());
    std::fs::write(dir.path().join("doc/product/grows.md"), &big).expect("grow");
    let ctx = context(&dir);
    let at_the_limit = head_texts(&ctx, Some("product/exact.md")).expect("exactly 1 MiB");
    assert_eq!(at_the_limit.before.as_deref(), Some(exact.as_str()));
    // HEAD's version is refused before git prints it.
    let committed = head_texts(&ctx, Some("product/big.md")).expect_err("HEAD's version");
    assert_eq!(committed.status, 413);
    // The current text is held up to 8 MiB, but a diff never ships more than doc_view does.
    let current = head_texts(&ctx, Some("product/grows.md")).expect_err("current version");
    assert_eq!(current.status, 413);
}

#[test]
fn a_git_failure_other_than_a_missing_path_is_a_500() {
    let dir = write_files(&[]);
    let ctx = context(&dir);
    // A `.git` file naming a missing directory: every git command fails with status 128.
    std::fs::write(
        dir.path().join(".git"),
        "gitdir: /missing-gitdir-for-test\n",
    )
    .expect("write");
    let refusal = head_texts(&ctx, Some("product/index.md")).expect_err("git fails");
    assert_eq!(refusal.status, 500);
    let body = String::from_utf8_lossy(&refusal.body);
    assert!(!body.contains("missing-gitdir-for-test"), "{body}");
}

#[test]
fn turn_diff_refuses_traversal_and_absolute_paths() {
    let dir = write_files(&[(".env", "SECRET=hunter2\n")]);
    let ctx = context(&dir);
    let turn = TurnId("session/paths".to_owned());
    text_store(&ctx).expect("store").begin_turn(&turn);
    end_turn(&ctx, &turn);
    for raw in ["../.env", "product/../../.env", "/etc/passwd", "..", "/"] {
        let refusal = turn_texts(&ctx, Some("session/paths"), Some(raw)).expect_err(raw);
        assert_eq!(refusal.status, 400, "{raw}");
        assert!(!String::from_utf8_lossy(&refusal.body).contains("hunter2"));
    }
}

#[test]
fn a_turn_that_has_not_ended_has_no_diff() {
    let dir = write_files(&[("doc/product/edited.md", "# E\n")]);
    let ctx = context(&dir);
    let store = text_store(&ctx).expect("store");
    let turn = TurnId("session/running".to_owned());
    store.begin_turn(&turn);
    std::fs::write(dir.path().join("doc/product/edited.md"), "# E after\n").expect("edit");
    // Without an end every pinned file would read as deleted.
    let running = turn_texts(&ctx, Some("session/running"), Some("product/edited.md"));
    assert_eq!(running.expect_err("not ended").status, 404);
    let index = turn_texts(&ctx, Some("session/running"), Some("product/index.md"));
    assert_eq!(index.expect_err("not ended").status, 404);
    end_turn(&ctx, &turn);
    let ended = turn_texts(&ctx, Some("session/running"), Some("product/edited.md"));
    assert_eq!(ended.expect("ended").after.as_deref(), Some("# E after\n"));
}

#[test]
fn a_turn_dropped_from_the_store_answers_404_not_a_deletion() {
    let dir = write_files(&[("doc/product/edited.md", "# E\n")]);
    let ctx = context(&dir);
    let store = text_store(&ctx).expect("store");
    let turn = TurnId("session/old".to_owned());
    store.begin_turn(&turn);
    std::fs::write(dir.path().join("doc/product/edited.md"), "# E after\n").expect("edit");
    end_turn(&ctx, &turn);
    assert!(turn_texts(&ctx, Some("session/old"), Some("product/edited.md")).is_ok());
    for n in 0..MAX_TURNS {
        store.begin_turn(&TurnId(format!("session/{n}")));
    }
    let dropped = turn_texts(&ctx, Some("session/old"), Some("product/edited.md"));
    assert_eq!(dropped.expect_err("dropped").status, 404);
}

#[test]
fn turn_diff_refuses_a_side_over_one_mib() {
    let dir = write_files(&[("doc/product/grows.md", "# G\n")]);
    let ctx = context(&dir);
    let turn = TurnId("session/big".to_owned());
    text_store(&ctx).expect("store").begin_turn(&turn);
    let big = "x".repeat(MAX_EDIT_BYTES + 1);
    std::fs::write(dir.path().join("doc/product/grows.md"), big).expect("grow");
    end_turn(&ctx, &turn);
    let grown = turn_texts(&ctx, Some("session/big"), Some("product/grows.md"));
    assert_eq!(grown.expect_err("after is over 1 MiB").status, 413);
}
