//! Regression tests for the bounds on `WorkDir::resolve`'s upward walk: a
//! `.git` ancestor with no structural validity must not bound or satisfy the
//! search, and a temp root nested inside a git checkout must stop the search
//! before it reaches the checkout's own live workspace.

use std::fs;

use tempfile::TempDir;

use super::super::{Layout, WorkDir};
use super::{bare_repo, plant_workspace};

#[test]
fn an_empty_git_directory_does_not_bound_the_walk() {
    let temp = TempDir::new().unwrap();
    let ancestor = temp.path().join("ancestor");
    fs::create_dir_all(ancestor.join(".git")).unwrap();
    plant_workspace(&ancestor, Layout::Nested);

    let inner = ancestor.join("not-a-repo").join("deeper");
    fs::create_dir_all(&inner).unwrap();

    let wd = WorkDir::new(&inner).unwrap();
    assert_eq!(
        wd.root(),
        inner.join(".loom").join("work"),
        "an ancestor .git with no HEAD must not bound the walk or be adopted"
    );
}

/// Positive control for the test above: the same ancestor with `.git/HEAD`
/// present IS adopted, proving non-adoption there comes from `HEAD`'s
/// absence, not from the directory layout.
#[test]
fn the_same_layout_with_head_present_is_adopted() {
    let temp = TempDir::new().unwrap();
    let ancestor = temp.path().join("ancestor");
    fs::create_dir_all(ancestor.join(".git")).unwrap();
    fs::write(ancestor.join(".git").join("HEAD"), "ref: refs/heads/main\n").unwrap();
    let workspace = plant_workspace(&ancestor, Layout::Nested);

    let inner = ancestor.join("not-a-repo").join("deeper");
    fs::create_dir_all(&inner).unwrap();

    let wd = WorkDir::new(&inner).unwrap();
    assert_eq!(
        wd.root().canonicalize().unwrap(),
        workspace.canonicalize().unwrap()
    );
}

/// A `.git` FILE (a linked worktree's pointer) counts on existence alone, so
/// it still bounds — and satisfies — the walk with no `HEAD` involved.
#[test]
fn a_git_file_still_bounds_the_walk() {
    let temp = TempDir::new().unwrap();
    let ancestor = temp.path().join("ancestor");
    fs::create_dir_all(&ancestor).unwrap();
    fs::write(ancestor.join(".git"), "gitdir: /somewhere\n").unwrap();
    let workspace = plant_workspace(&ancestor, Layout::Nested);

    let inner = ancestor.join("not-a-repo").join("deeper");
    fs::create_dir_all(&inner).unwrap();

    let wd = WorkDir::new(&inner).unwrap();
    assert_eq!(
        wd.root().canonicalize().unwrap(),
        workspace.canonicalize().unwrap()
    );
}

/// Incident: a stage ran `cargo test` with `TMPDIR` set to a directory nested
/// inside this checkout (`<repo>/loom/target/tmp`). Every test's `TempDir`
/// then landed under that `TMPDIR`, which sits BELOW the checkout's `.git` —
/// so the repo-root bound alone never stopped the upward search before it
/// reached the checkout's live `.loom/work`, and tests wrote fixtures into
/// the running orchestrator's own state. The temp-root bound stops the walk
/// at the temp root itself, without disturbing a workspace a test plants
/// inside its own tempdir.
#[test]
fn a_temp_root_nested_in_a_checkout_never_adopts_the_checkouts_workspace() {
    let temp = TempDir::new().unwrap();
    let repo_root = bare_repo(&temp);
    let live = plant_workspace(&repo_root, Layout::Nested);

    // Mirrors the incident: a TMPDIR under the checkout's own target/ dir.
    let temp_root = repo_root.join("loom").join("target").join("tmp");
    let base = temp_root.join(".tmpX");
    fs::create_dir_all(&base).unwrap();
    let temp_root = temp_root.canonicalize().unwrap();

    let wd = WorkDir::resolve(&base, Some(&temp_root));
    assert_eq!(
        wd.root(),
        base.join(".loom").join("work"),
        "a base under a TMPDIR nested in the checkout must never adopt the checkout's live workspace"
    );

    // Positive control: with no temp bound, the same base DOES adopt the
    // checkout's workspace — proving the bound above, not something else,
    // is what isolates it.
    let wd_unbounded = WorkDir::resolve(&base, None);
    assert_eq!(
        wd_unbounded.root().canonicalize().unwrap(),
        live.canonicalize().unwrap(),
        "without the temp bound the walk must still reach the repo root's workspace"
    );
}

#[test]
fn a_workspace_planted_inside_the_temp_root_still_resolves() {
    let temp = TempDir::new().unwrap();
    let temp_root = temp.path().join("tmp-root");
    fs::create_dir_all(&temp_root).unwrap();
    let temp_root = temp_root.canonicalize().unwrap();

    let proj = temp_root.join("proj");
    fs::create_dir_all(proj.join(".git")).unwrap();
    fs::write(proj.join(".git").join("HEAD"), "ref: refs/heads/main\n").unwrap();
    let workspace = plant_workspace(&proj, Layout::Nested);

    let base = proj.join("sub");
    fs::create_dir_all(&base).unwrap();

    let wd = WorkDir::resolve(&base, Some(&temp_root));
    assert_eq!(
        wd.root().canonicalize().unwrap(),
        workspace.canonicalize().unwrap(),
        "a workspace planted inside the caller's own tempdir must still resolve"
    );
}

#[test]
fn base_equal_to_the_temp_root_does_not_walk_above_it() {
    let temp = TempDir::new().unwrap();
    let repo_root = bare_repo(&temp);
    plant_workspace(&repo_root, Layout::Nested);

    let temp_root = repo_root.join("loom").join("target").join("tmp");
    fs::create_dir_all(&temp_root).unwrap();
    let temp_root = temp_root.canonicalize().unwrap();

    let wd = WorkDir::resolve(&temp_root, Some(&temp_root));
    assert_eq!(
        wd.root(),
        temp_root.join(".loom").join("work"),
        "a base equal to the temp root must not walk above it to adopt the repo's workspace"
    );
}
