//! `uncommitted_reasons` against temporary repositories: the `-z` records of paths with spaces
//! and of a staged rename, and a project root below the repository's top level.

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::uncommitted_reasons;
use crate::test_git::{git, TestRepo};

const SPACED: &str = "doc/plans/briefs/a b.md";
const MOVED: &str = "doc/plans/briefs/d e.md";
const KEPT: &str = "doc/plans/briefs/c.md";
const UNTRACKED: &str = "not committed (untracked, ignored, or staged only)";

/// A repository whose one commit tracks `SPACED` and `KEPT` under `<top>/<below>`, with that
/// directory as the project root. `below` is empty for a project at the top level.
fn repo(below: &str) -> (TempDir, PathBuf) {
    let temp = TempDir::new().unwrap();
    let top = temp.path();
    TestRepo::builder().no_seed().init_in(top);
    let root = top.join(below);
    for rel in [SPACED, KEPT] {
        fs::create_dir_all(root.join(rel).parent().unwrap()).unwrap();
        fs::write(root.join(rel), format!("{rel}\n")).unwrap();
    }
    git(top, &["add", "."]);
    git(top, &["commit", "-q", "-m", "base"]);
    (temp, root)
}

/// Stage the rename of `SPACED` to `MOVED` in the project at `root`.
fn stage_rename(root: &Path) {
    git(root, &["mv", SPACED, MOVED]);
}

fn reasons(root: &Path) -> Vec<Option<&'static str>> {
    let paths = [SPACED, KEPT, MOVED].map(Path::new);
    uncommitted_reasons(root, &paths).unwrap()
}

#[test]
fn a_staged_rename_of_a_path_with_spaces_marks_both_sides() {
    let (_temp, root) = repo("");
    assert_eq!(reasons(&root), [None, None, Some(UNTRACKED)]);
    stage_rename(&root);
    assert_eq!(
        reasons(&root),
        [Some("uncommitted changes"), None, Some(UNTRACKED)]
    );
}

#[test]
fn a_project_below_the_top_level_reads_status_paths_relative_to_itself() {
    let (_temp, root) = repo("my project");
    stage_rename(&root);
    fs::write(root.join(KEPT), "edited\n").unwrap();
    assert_eq!(
        reasons(&root),
        [
            Some("uncommitted changes"),
            Some("uncommitted changes"),
            Some(UNTRACKED)
        ]
    );
}

#[test]
fn changed_paths_keeps_both_rename_paths_with_spaces_below_the_top_level() {
    let (_temp, root) = repo("my project");
    stage_rename(&root);
    let git = crate::git::worktree::WorktreeGit::discovered(&root);
    let changed = super::git_state::changed_paths(&git, &["doc/plans/briefs".to_owned()]).unwrap();
    assert_eq!(changed, [SPACED.to_owned(), MOVED.to_owned()].into());
}
