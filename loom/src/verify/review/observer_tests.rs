//! [`checkout_git`] gives a process outside the stage's sandbox the git of
//! the checkout holding a directory: pinned to the registered git directory
//! for a path in a stage worktree, whatever `.git` entries the stage made in
//! it, and as git finds it for any other checkout.

use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

use super::{checkout_git, local_git};
use crate::git::worktree::WorktreeGit;
use crate::verify::contracts::test_support::{contract_worktree, plant_foreign_git_dir};
use crate::verify::duplicate_detection::graph::{changed_files, repository_root};

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {output:?}");
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

/// The positive control runs last: git that follows the repointed `.git`
/// file reports the planted git directory's HEAD and common directory, so
/// the pinned answers before come from the pin.
#[test]
fn a_repointed_git_file_moves_neither_the_toplevel_nor_the_git_directories() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    let worktree = contract_worktree(&repo, "s1");
    let honest_head = git(&worktree, &["rev-parse", "HEAD"]);
    let marker = plant_foreign_git_dir(&worktree);
    let foreign = tmp.path().join("foreign.git");
    let forged = git(&foreign, &["commit-tree", "HEAD^{tree}", "-m", "forged"]);
    git(&foreign, &["update-ref", "refs/heads/loom/s1", &forged]);
    let nested = worktree.join("tests");

    let pinned = checkout_git(&nested).unwrap();

    assert_eq!(pinned.work_tree(), worktree.canonicalize().unwrap());
    let common = ["rev-parse", "--path-format=absolute", "--git-common-dir"];
    assert_eq!(
        pinned.run_checked(&common).unwrap().trim(),
        repo.join(".git").canonicalize().unwrap().to_str().unwrap()
    );
    let head = pinned.run_checked(&["rev-parse", "HEAD"]).unwrap();
    assert_eq!(head.trim(), honest_head);
    pinned.run_checked(&["status", "--porcelain"]).unwrap();
    assert!(!marker.exists(), "pinned git ran the stage's clean filter");

    assert_eq!(git(&worktree, &["rev-parse", "HEAD"]), forged);
    assert_eq!(
        git(&worktree, &common),
        foreign.canonicalize().unwrap().to_str().unwrap()
    );
    git(&worktree, &["status", "--porcelain"]);
    assert!(marker.exists(), "the fixture's filter never runs");
}

/// Make `dir` a repository of its own, its git directory at `git_dir` behind a
/// `.git` file when one is given, whose clean filter creates `marker`. Its
/// tracked `a.txt` is left stat-dirty, so git that reads this repository
/// re-hashes it through the filter.
fn plant_nested_repository(dir: &Path, git_dir: Option<&Path>, marker: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    match git_dir {
        Some(git_dir) => git(
            dir,
            &[
                "init",
                "-q",
                "--separate-git-dir",
                git_dir.to_str().unwrap(),
            ],
        ),
        None => git(dir, &["init", "-q"]),
    };
    std::fs::write(dir.join("a.txt"), "base\n").unwrap();
    git(dir, &["add", "a.txt"]);
    git(dir, &["commit", "-q", "-m", "planted"]);
    let filter = format!("touch '{}'; cat", marker.display());
    git(dir, &["config", "filter.evil.clean", &filter]);
    std::fs::write(dir.join(".gitattributes"), "* filter=evil\n").unwrap();
    git(dir, &["read-tree", "HEAD"]);
    std::fs::write(dir.join("a.txt"), "changed\n").unwrap();
}

/// What the stage worktree's own git answers through `found`: its toplevel
/// and HEAD, with a status run that would re-hash a planted stat-dirty file.
fn assert_reads_the_worktree(found: &WorktreeGit, worktree: &Path, honest_head: &str) {
    assert_eq!(found.work_tree(), worktree.canonicalize().unwrap());
    let head = found.run_checked(&["rev-parse", "HEAD"]).unwrap();
    assert_eq!(head.trim(), honest_head);
    found.run_checked(&["status", "--porcelain"]).unwrap();
}

/// A `.git` file and a `.git` directory the stage creates below its worktree
/// root choose no host process's git: every path through `checkout_git` and
/// `local_git` reads the registered worktree. The positive control runs last.
#[test]
fn a_repository_planted_below_the_worktree_root_chooses_no_git() {
    let tmp = TempDir::new().unwrap();
    let worktree = contract_worktree(&tmp.path().join("repo"), "s1");
    let honest_head = git(&worktree, &["rev-parse", "HEAD"]);
    let (gitfile_dir, gitdir_dir) = (worktree.join("loom"), worktree.join("tests"));
    let gitfile_marker = tmp.path().join("gitfile-filter-ran");
    let gitdir_marker = tmp.path().join("gitdir-filter-ran");
    let nested_git = tmp.path().join("nested.git");
    plant_nested_repository(&gitfile_dir, Some(&nested_git), &gitfile_marker);
    plant_nested_repository(&gitdir_dir, None, &gitdir_marker);

    for dir in [&gitfile_dir, &gitdir_dir] {
        assert_reads_the_worktree(&checkout_git(dir).unwrap(), &worktree, &honest_head);
        assert_reads_the_worktree(&local_git(dir).unwrap(), &worktree, &honest_head);
        assert_eq!(
            repository_root(dir).unwrap(),
            worktree.canonicalize().unwrap()
        );
        changed_files(dir, "main", false).unwrap();
    }
    assert!(
        !gitfile_marker.exists(),
        "host git ran the planted .git file's filter"
    );
    assert!(
        !gitdir_marker.exists(),
        "host git ran the planted .git directory's filter"
    );

    git(&gitfile_dir, &["status", "--porcelain"]);
    git(&gitdir_dir, &["status", "--porcelain"]);
    assert!(
        gitfile_marker.exists(),
        "the .git file fixture's filter never runs"
    );
    assert!(
        gitdir_marker.exists(),
        "the .git directory fixture's filter never runs"
    );
}

/// A stage can lay out a project with a stage worktree of its own inside its
/// worktree, whose registered git directory it configured. The outer, real
/// stage worktree pins git; the positive control, run last, shows the inner
/// one's pin runs the filter.
#[test]
fn a_stage_worktree_planted_inside_a_stage_worktree_chooses_no_git() {
    let tmp = TempDir::new().unwrap();
    let worktree = contract_worktree(&tmp.path().join("repo"), "s1");
    let honest_head = git(&worktree, &["rev-parse", "HEAD"]);
    let fake_project = worktree.join("fake");
    let marker = tmp.path().join("filter-ran");
    plant_nested_repository(&fake_project, None, &marker);
    std::fs::create_dir_all(fake_project.join(".loom/work")).unwrap();
    git(
        &fake_project,
        &["worktree", "add", "-q", "-b", "loom/s2", ".worktrees/s2"],
    );
    let fake_worktree = fake_project.join(".worktrees/s2");
    std::fs::write(fake_worktree.join(".gitattributes"), "* filter=evil\n").unwrap();
    git(&fake_worktree, &["read-tree", "HEAD"]);
    std::fs::write(fake_worktree.join("a.txt"), "changed\n").unwrap();
    // Setting the fixture up may already have run the filter.
    std::fs::remove_file(&marker).ok();

    assert_reads_the_worktree(
        &checkout_git(&fake_worktree).unwrap(),
        &worktree,
        &honest_head,
    );
    assert_reads_the_worktree(&local_git(&fake_worktree).unwrap(), &worktree, &honest_head);
    assert!(
        !marker.exists(),
        "host git ran the planted worktree's filter"
    );

    let planted = WorktreeGit::pinned(&fake_project, &fake_worktree).unwrap();
    planted.run_checked(&["status", "--porcelain"]).unwrap();
    assert!(marker.exists(), "the planted worktree's filter never runs");
}

#[test]
fn a_checkout_that_is_no_stage_worktree_is_read_as_git_finds_it() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("plain");
    let nested = root.join("a/b");
    std::fs::create_dir_all(&nested).unwrap();
    git(&root, &["init", "-q", "-b", "main"]);

    let found = checkout_git(&nested).unwrap();

    assert_eq!(found.work_tree(), root.canonicalize().unwrap());
    let toplevel = found
        .run_checked(&["rev-parse", "--show-toplevel"])
        .unwrap();
    assert_eq!(
        toplevel.trim(),
        root.canonicalize().unwrap().to_str().unwrap()
    );
}

#[test]
fn a_directory_inside_no_checkout_is_refused() {
    let tmp = TempDir::new().unwrap();

    let error = checkout_git(tmp.path()).unwrap_err();

    assert!(format!("{error:#}").contains("inside no git checkout"));
}
