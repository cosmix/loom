//! The commit `snapshot_changes` writes against the one real `git stash create`
//! writes, for each shape a checkout's tracked changes can take.

use std::fs::{self, File};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::Path;
use std::time::{Duration, SystemTime};
use tempfile::TempDir;

use super::*;
use crate::git::merge::test_support::{
    commit_file, git_ok, git_out, init_repo, isolated_git, stage_branch,
};

mod end_to_end;

const LINES: &str = "l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8\nl9\nl10\n";

/// `f.txt`, `dir/b.txt`, `gone/c.txt`, `run.sh` and `out/d.txt` committed.
fn repo() -> TempDir {
    let repo = init_repo();
    let root = repo.path();
    for dir in ["dir", "gone", "out"] {
        fs::create_dir(root.join(dir)).unwrap();
    }
    commit_file(root, "f.txt", LINES, "f");
    commit_file(root, "dir/b.txt", "b\n", "b");
    commit_file(root, "gone/c.txt", "c\n", "c");
    commit_file(root, "run.sh", "#!/bin/sh\n", "run");
    commit_file(root, "out/d.txt", "d\n", "d");
    repo
}

fn write(root: &Path, path: &str, text: &str) {
    fs::write(root.join(path), text).unwrap();
}

/// Everything of a stash commit an operator can observe.
fn shape(root: &Path, commit: &str) -> [String; 6] {
    let of = |revision: String| git_out(root, &["rev-parse", &revision]);
    let subject = |revision: &str| git_out(root, &["log", "-1", "--format=%s", revision]);
    [
        of(format!("{commit}^{{tree}}")),
        of(format!("{commit}^1")),
        of(format!("{commit}^2^{{tree}}")),
        of(format!("{commit}^2^1")),
        subject(commit),
        subject(&format!("{commit}^2")),
    ]
}

fn assert_matches_git_stash(root: &Path, case: &str) {
    let ours = snapshot_changes(root).unwrap_or_else(|error| panic!("{case}: {error:#}"));
    let theirs = git_out(root, &["stash", "create"]);
    match ours {
        None if theirs.is_empty() => {}
        // Git saw a difference it records nothing of: a directory where the index
        // has a file or a gitlink at another commit. There is nothing to restore.
        None => {
            let head_tree = git_out(root, &["rev-parse", "HEAD^{tree}"]);
            for tree in [format!("{theirs}^{{tree}}"), format!("{theirs}^2^{{tree}}")] {
                assert_eq!(git_out(root, &["rev-parse", &tree]), head_tree, "{case}");
            }
        }
        Some(snapshot) => {
            assert_ne!(theirs, "", "{case}: only the snapshot saw changes");
            assert_eq!(
                shape(root, &snapshot.commit),
                shape(root, &theirs),
                "{case}"
            );
        }
    }
}

fn staged_edit_then_more(root: &Path) {
    write(root, "f.txt", "staged\n");
    git_ok(root, &["add", "f.txt"]);
    write(root, "f.txt", "staged and more\n");
}

fn staged_new_then_removed(root: &Path) {
    write(root, "new.txt", "new\n");
    git_ok(root, &["add", "new.txt"]);
    fs::remove_file(root.join("new.txt")).unwrap();
}

fn file_becomes_symlink(root: &Path) {
    fs::remove_file(root.join("f.txt")).unwrap();
    symlink("dir/b.txt", root.join("f.txt")).unwrap();
}

fn file_becomes_directory(root: &Path) {
    fs::remove_file(root.join("f.txt")).unwrap();
    fs::create_dir(root.join("f.txt")).unwrap();
    write(root, "f.txt/inside", "untracked\n");
}

/// With another change, the stash path's `reset --hard` follows, and it deletes
/// the directory, as `git stash push` does.
fn file_becomes_directory_beside_an_edit(root: &Path) {
    file_becomes_directory(root);
    write(root, "dir/b.txt", "edited\n");
}

fn directory_becomes_file(root: &Path) {
    fs::remove_dir_all(root.join("gone")).unwrap();
    write(root, "gone", "a file now\n");
}

/// A path whose bytes are not UTF-8, and one holding a newline.
fn awkward_names(root: &Path) {
    let latin1 = std::ffi::OsStr::from_bytes(b"caf\xe9.txt");
    fs::write(root.join(latin1), "x\n").unwrap();
    write(root, "new\nline.txt", "y\n");
    git_ok(root, &["add", "."]);
    fs::write(root.join(latin1), "x2\n").unwrap();
    write(root, "new\nline.txt", "y2\n");
}

fn cleaned_by_a_filter(root: &Path) {
    write(root, ".gitattributes", "*.up filter=up\n");
    git_ok(root, &["config", "filter.up.clean", "tr a-z A-Z"]);
    write(root, "x.up", "lower\n");
    git_ok(root, &["add", ".gitattributes", "x.up"]);
    git_ok(root, &["commit", "-m", "filtered"]);
    write(root, "x.up", "changed\n");
}

/// Line endings that normalize away: the file differs on disk, not in git.
fn line_endings_normalized_away(root: &Path) {
    git_ok(root, &["config", "core.autocrlf", "input"]);
    write(root, "f.txt", &LINES.replace('\n', "\r\n"));
    write(root, "dir/b.txt", "b2\n");
}

fn split_index(root: &Path) {
    git_ok(root, &["update-index", "--split-index"]);
    staged_edit_then_more(root);
}

fn index_version_4(root: &Path) {
    git_ok(root, &["update-index", "--index-version", "4"]);
    staged_edit_then_more(root);
}

fn gitlink_moved(root: &Path) {
    let sub = root.join("sub");
    fs::create_dir(&sub).unwrap();
    git_ok(&sub, &["init", "-q", "-b", "main"]);
    git_ok(&sub, &["config", "user.email", "t@t.com"]);
    git_ok(&sub, &["config", "user.name", "t"]);
    commit_file(&sub, "s.txt", "1\n", "one");
    git_ok(root, &["add", "sub"]);
    git_ok(root, &["commit", "-m", "gitlink"]);
    commit_file(&sub, "s.txt", "2\n", "two");
}

fn gitlink_dirty_inside(root: &Path) {
    gitlink_moved(root);
    git_ok(root, &["add", "sub"]);
    git_ok(root, &["commit", "-m", "at the moved commit"]);
    write(&root.join("sub"), "s.txt", "dirty inside\n");
}

/// The test in git's own t3903: staged, then marked skip-worktree and removed.
fn staged_skip_worktree(root: &Path) {
    write(root, "out/d.txt", "changed\n");
    git_ok(root, &["add", "out/d.txt"]);
    git_ok(root, &["update-index", "--skip-worktree", "out/d.txt"]);
    fs::remove_file(root.join("out/d.txt")).unwrap();
}

/// A cone sparse checkout whose index holds a change to an out-of-cone path:
/// `out/d.txt` is staged at "d2" and absent from disk.
fn sparse_cone(root: &Path, args: &[&str]) {
    commit_file(root, "out/d.txt", "d2\n", "out-of-cone");
    git_ok(root, &["reset", "--soft", "HEAD~1"]);
    let mut set = vec!["sparse-checkout", "set", "--cone"];
    set.extend(args);
    set.push("dir");
    git_ok(root, &set);
    assert!(!root.join("out/d.txt").exists());
}

type Shape = (&'static str, fn(&Path));

const CASES: &[Shape] = &[
    ("clean", |_| {}),
    ("untracked only", |root| write(root, "new.txt", "new\n")),
    ("unstaged edit", |root| write(root, "f.txt", "edited\n")),
    ("staged edit", |root| {
        write(root, "f.txt", "staged\n");
        git_ok(root, &["add", "f.txt"]);
    }),
    ("staged edit, edited again", staged_edit_then_more),
    ("staged edit, undone on disk", |root| {
        write(root, "f.txt", "staged\n");
        git_ok(root, &["add", "f.txt"]);
        write(root, "f.txt", LINES);
    }),
    ("removed on disk", |root| {
        fs::remove_file(root.join("f.txt")).unwrap();
    }),
    ("staged removal", |root| {
        git_ok(root, &["rm", "-q", "f.txt"])
    }),
    ("staged removal, file kept", |root| {
        git_ok(root, &["rm", "-q", "--cached", "f.txt"]);
    }),
    ("staged new file", |root| {
        write(root, "new.txt", "new\n");
        git_ok(root, &["add", "new.txt"]);
    }),
    ("staged new file, edited", |root| {
        write(root, "new.txt", "new\n");
        git_ok(root, &["add", "new.txt"]);
        write(root, "new.txt", "new and more\n");
    }),
    ("staged new file, removed", staged_new_then_removed),
    ("staged rename", |root| {
        git_ok(root, &["mv", "f.txt", "renamed.txt"]);
    }),
    ("mode change", |root| {
        let mode = fs::Permissions::from_mode(0o755);
        fs::set_permissions(root.join("run.sh"), mode).unwrap();
    }),
    ("file becomes symlink", file_becomes_symlink),
    ("file becomes directory", file_becomes_directory),
    (
        "file becomes directory, with an edit",
        file_becomes_directory_beside_an_edit,
    ),
    ("directory becomes file", directory_becomes_file),
    ("assume-unchanged edit", |root| {
        git_ok(root, &["update-index", "--assume-unchanged", "f.txt"]);
        write(root, "f.txt", "hidden edit\n");
        write(root, "dir/b.txt", "visible edit\n");
    }),
    ("tracked file that is ignored", |root| {
        write(root, ".git/info/exclude", "f.txt\n");
        write(root, "f.txt", "edited\n");
    }),
    ("awkward names", awkward_names),
    ("clean filter", cleaned_by_a_filter),
    ("line endings", line_endings_normalized_away),
    ("split index", split_index),
    ("index version 4", index_version_4),
    ("gitlink at a moved commit", gitlink_moved),
    ("gitlink at a moved commit, with an edit", |root| {
        gitlink_moved(root);
        write(root, "dir/b.txt", "edited\n");
    }),
    ("gitlink dirty inside", gitlink_dirty_inside),
    ("staged skip-worktree", staged_skip_worktree),
    ("sparse cone", |root| {
        sparse_cone(root, &[]);
        write(root, "dir/b.txt", "b2\n");
    }),
    ("sparse index", |root| {
        sparse_cone(root, &["--sparse-index"]);
        write(root, "dir/b.txt", "b2\n");
    }),
    ("no index file", |root| {
        fs::remove_file(root.join(".git/index")).unwrap();
    }),
];

#[test]
fn the_snapshot_is_the_commit_git_stash_writes() {
    for (case, prepare) in CASES {
        let repo = repo();
        prepare(repo.path());
        assert_matches_git_stash(repo.path(), case);
    }
}

#[test]
fn what_git_stash_refuses_the_snapshot_refuses() {
    let intent_to_add: fn(&Path) = |root| {
        write(root, "new.txt", "new\n");
        git_ok(root, &["add", "-N", "new.txt"]);
    };
    let unmerged: fn(&Path) = |root| {
        stage_branch(root, "s", &[("f.txt", "theirs\n")]);
        commit_file(root, "f.txt", "ours\n", "ours");
        assert!(!isolated_git(root, &["merge", "loom/s"]).status.success());
    };
    for (case, prepare) in [("intent to add", intent_to_add), ("unmerged", unmerged)] {
        let repo = repo();
        let root = repo.path();
        prepare(root);
        assert!(snapshot_changes(root).is_err(), "{case}");
        let git = isolated_git(root, &["stash", "create"]);
        assert!(!git.status.success(), "{case}: git stash accepted it");
    }
}

/// An edit of the same size in the second the index was written. Only the
/// index's own modification time tells git to compare its content; a private
/// copy with a later time would read it as clean, and the reset would lose it.
#[test]
fn a_racy_edit_is_seen_through_the_private_index() {
    let repo = repo();
    let root = repo.path();
    let earlier = SystemTime::now() - Duration::from_secs(10);
    let set_mtime = |path: &Path| {
        let file = File::options().write(true).open(path).unwrap();
        file.set_modified(earlier).unwrap();
    };
    git_ok(root, &["config", "core.trustctime", "false"]);
    write(root, "dir/b.txt", "bbb\n");
    set_mtime(&root.join("dir/b.txt"));
    git_ok(root, &["add", "dir/b.txt"]);
    set_mtime(&root.join(".git/index"));
    write(root, "dir/b.txt", "ccc\n");
    set_mtime(&root.join("dir/b.txt"));

    assert_matches_git_stash(root, "racy edit");
    let snapshot = snapshot_changes(root).unwrap().expect("the edit is seen");
    let stored = git_out(root, &["show", &format!("{}:dir/b.txt", snapshot.commit)]);
    assert_eq!(stored, "ccc");
}

/// Reading the checkout needs no lock, so a failed reset can still be told from
/// one that changed something. Storing the entry stops at a held lock, before there
/// is anything for the operator to clean up.
#[test]
fn a_held_index_lock_stops_the_store_and_not_the_read() {
    let repo = repo();
    let root = repo.path();
    write(root, "f.txt", "edited\n");
    write(root, ".git/index.lock", "");

    let snapshot = snapshot_changes(root).unwrap().expect("a change");

    let stored = git_out(root, &["show", &format!("{}:f.txt", snapshot.commit)]);
    assert_eq!(stored, "edited");
    assert!(checkout_unchanged(root, &snapshot));
    assert!(store_unchanged(root, &snapshot).is_err());
    assert_eq!(git_out(root, &["stash", "list"]), "");
}

#[test]
fn an_edit_after_the_snapshot_is_a_changed_checkout() {
    let repo = repo();
    let root = repo.path();
    write(root, "f.txt", "edited\n");
    let snapshot = snapshot_changes(root).unwrap().expect("a change");

    write(root, "f.txt", "edited again\n");

    assert!(!checkout_unchanged(root, &snapshot));
}
