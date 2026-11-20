//! A stage whose base loom can read, for tests that admit a narrowing on
//! recorded base evidence: a base run counts only at the stage's current base.

use std::path::Path;

use crate::test_git::{commit_file, git, TestRepo};

/// Make `root`, which holds the state directory `<root>/.loom/work` whose
/// `config.toml` names `main` as the base branch, a repository whose `main`
/// has one commit and whose stage `stage_id` has its worktree at
/// `.worktrees/<stage_id>` on `loom/<stage_id>`, with the checkout identity
/// loom pins git to recorded. Returns the stage's base.
pub(crate) fn stage_with_base(root: &Path, stage_id: &str) -> String {
    let root = root.canonicalize().expect("canonical repository root");
    TestRepo::builder().branch("main").no_seed().init_in(&root);
    let base = commit_file(&root, ".gitignore", ".loom/\n.worktrees/\n", "initial");
    let worktree = root.join(".worktrees").join(stage_id);
    let branch = format!("loom/{stage_id}");
    let worktree = worktree.to_str().expect("UTF-8 worktree path");
    git(&root, &["worktree", "add", "-q", "-b", &branch, worktree]);
    let state = root.join(".loom").join("work");
    crate::git::worktree::capture_main_identity(&root, &state).expect("capture identity");
    base
}

/// The base of a stage [`stage_with_base`] made in the repository holding
/// the state directory `work_dir`: the commit `main` holds.
pub(crate) fn base_of(work_dir: &Path) -> String {
    git(&work_dir.join("..").join(".."), &["rev-parse", "main"])
}

/// Move `main` of the repository at `root` one commit on and merge it into
/// stage `stage_id`'s branch, as a stage taking in a newer base does.
/// Returns the stage's new base.
pub(crate) fn advance_base(root: &Path, stage_id: &str) -> String {
    let root = root.canonicalize().expect("canonical repository root");
    let base = commit_file(&root, "main.txt", "newer\n", "newer base");
    let worktree = root.join(".worktrees").join(stage_id);
    git(&worktree, &["merge", "-q", "--ff-only", "main"]);
    base
}
