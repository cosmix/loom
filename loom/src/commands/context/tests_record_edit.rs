//! Tests for `loom context record-edit`.
//!
//! The path rules are the whole risk surface here: getting the `.worktrees`
//! polarity backwards silently discards every edit an orchestrated stage makes,
//! and a missed escape lets a hook payload name a file outside the checkout.

use super::*;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// A main checkout with one linked worktree under it, and `<worktree>/src/`
/// present. Returns `(temp, main checkout, worktree root)`.
fn checkout_with_worktree() -> (TempDir, PathBuf, PathBuf) {
    let temp = TempDir::new().unwrap();
    let main = temp.path().join("main");
    let worktree = main.join(".worktrees").join("stage-a");
    fs::create_dir_all(worktree.join("src")).unwrap();
    (temp, main, worktree)
}

#[test]
fn records_a_file_that_lives_below_the_worktrees_directory() {
    let (_temp, _main, worktree) = checkout_with_worktree();
    let edited = worktree.join("src/x.rs");

    assert_eq!(
        relative_to_root(&worktree, &edited, false).as_deref(),
        Some("src/x.rs"),
        "an orchestrated stage's edits sit below .worktrees/<stage>/ and must \
         still be recorded when the worktree is the active root"
    );
}

#[test]
fn worktrees_is_administrative_only_when_scanning_the_main_checkout() {
    let (_temp, main, worktree) = checkout_with_worktree();
    let edited = worktree.join("src/x.rs");

    assert!(
        relative_to_root(&main, &edited, true).is_none(),
        "from the main checkout, another stage's worktree is machinery"
    );
    assert!(
        !is_ignored(Path::new(".worktrees/stage-a/src/x.rs"), false),
        "inside a worktree every file sits below .worktrees/<stage>/"
    );
    assert!(is_ignored(Path::new(".worktrees/stage-a/src/x.rs"), true));
}

#[test]
fn skips_loom_state_derived_data_and_build_output() {
    for skipped in [
        ".work/stages/01-stage-a.md",
        ".loom/work/stages/01-stage-a.md",
        ".loom/cache/context-v1/catalog.json",
        "doc/loom/knowledge/architecture.md",
        "loom/target/debug/loom",
        "web/node_modules/left-pad/index.js",
        ".venv/lib/python3.12/site.py",
    ] {
        assert!(
            is_ignored(Path::new(skipped), false),
            "expected {skipped} to be skipped"
        );
    }
    assert!(!is_ignored(Path::new("loom/src/commands/mod.rs"), false));
}

#[test]
fn rejects_a_parent_directory_escape() {
    let (_temp, _main, worktree) = checkout_with_worktree();

    assert!(
        relative_to_root(&worktree, Path::new("../../etc/passwd"), false).is_none(),
        "a relative escape must be rejected, not clamped to the root"
    );
    assert!(relative_to_root(&worktree, Path::new("/etc/passwd"), false).is_none());
    assert!(relative_to_root(&worktree, Path::new("src/../../../etc/passwd"), false).is_none());
}

#[test]
fn accepts_an_interior_parent_hop_that_stays_inside_the_root() {
    let (_temp, _main, worktree) = checkout_with_worktree();

    assert_eq!(
        relative_to_root(&worktree, Path::new("src/../src/x.rs"), false).as_deref(),
        Some("src/x.rs")
    );
}

#[test]
fn rejects_a_path_reached_through_a_symlinked_directory() {
    let (_temp, _main, worktree) = checkout_with_worktree();
    let linked = worktree.join("linked");
    std::os::unix::fs::symlink(worktree.join("src"), &linked).unwrap();

    assert!(
        relative_to_root(&worktree, &linked.join("x.rs"), false).is_none(),
        "a symlinked component can point anywhere; reject rather than follow"
    );
    assert!(
        relative_to_root(&worktree, &worktree.join("src/fresh.rs"), false).is_some(),
        "a not-yet-created file has no metadata and must still be recorded"
    );
}

#[test]
fn recording_twice_deduplicates_and_keeps_earlier_paths() {
    let temp = TempDir::new().unwrap();
    let overlay = temp.path().join("overlay");

    merge_dirty_paths(
        &overlay,
        ["src/b.rs".to_string(), "src/a.rs".to_string()].into(),
    )
    .unwrap();
    merge_dirty_paths(
        &overlay,
        ["src/a.rs".to_string(), "src/c.rs".to_string()].into(),
    )
    .unwrap();

    assert_eq!(
        stored_paths(&overlay),
        vec!["src/a.rs", "src/b.rs", "src/c.rs"],
        "each invocation sees one edit and must not erase the ones before it"
    );
}

#[test]
fn a_malformed_record_is_replaced_rather_than_fatal() {
    let temp = TempDir::new().unwrap();
    let overlay = temp.path().join("overlay");
    fs::create_dir_all(&overlay).unwrap();
    fs::write(overlay.join("dirty-paths.json"), "{ not json").unwrap();

    merge_dirty_paths(&overlay, ["src/a.rs".to_string()].into()).unwrap();

    assert_eq!(stored_paths(&overlay), vec!["src/a.rs"]);
}

#[test]
fn the_stored_record_holds_paths_only() {
    let temp = TempDir::new().unwrap();
    let overlay = temp.path().join("overlay");

    merge_dirty_paths(&overlay, ["src/a.rs".to_string()].into()).unwrap();

    let raw = fs::read_to_string(overlay.join("dirty-paths.json")).unwrap();
    let document: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let keys: Vec<&String> = document.as_object().unwrap().keys().collect();
    assert_eq!(
        keys,
        vec!["paths", "recorded_at"],
        "no field may carry file contents, diffs, or tool results"
    );
    assert!(document["paths"].is_array());
}

#[test]
fn a_linked_worktree_is_not_the_main_checkout() {
    let temp = TempDir::new().unwrap();
    let main = temp.path().canonicalize().unwrap().join("main");
    let worktree = main.join(".worktrees").join("stage-a");
    let main_work = main.join(".loom").join("work");
    fs::create_dir_all(&main_work).unwrap();
    // `WorkDir::new` keys resolution on `config.toml`'s presence, not
    // directory existence, so a real (if empty) one is needed here for
    // `main` to be recognised as a workspace at all.
    fs::write(main_work.join("config.toml"), "").unwrap();
    fs::create_dir_all(&worktree).unwrap();
    fs::create_dir_all(worktree.join(".loom")).unwrap();
    std::os::unix::fs::symlink("../../../.loom/work", worktree.join(".loom").join("work")).unwrap();

    let from_main = crate::fs::work_dir::WorkDir::new(&main).unwrap();
    assert!(is_main_checkout(&from_main, &main));

    let from_worktree = crate::fs::work_dir::WorkDir::new(&worktree).unwrap();
    assert!(
        !is_main_checkout(&from_worktree, &worktree),
        "inside a worktree the main root differs, which is what keeps \
         .worktrees/<stage>/ paths recordable"
    );
}

#[test]
fn a_malformed_invocation_is_the_only_error() {
    assert!(record_edit("", &[]).is_err(), "an empty stage is malformed");
    assert!(record_edit("../escape", &[]).is_err());
    assert!(
        record_edit("stage-a", &[]).is_ok(),
        "nothing to record is not a failure"
    );
}

/// The paths persisted for a stage, as the on-disk JSON reports them.
fn stored_paths(overlay: &Path) -> Vec<String> {
    let raw = fs::read_to_string(overlay.join("dirty-paths.json")).unwrap();
    let document: serde_json::Value = serde_json::from_str(&raw).unwrap();
    document["paths"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_string())
        .collect()
}
