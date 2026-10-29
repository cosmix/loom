//! Claude Code's own worktree checkouts in `.claude/worktrees/` are not session configuration.

use std::fs::{self, File};
use std::os::unix::fs::symlink;

use tempfile::tempdir;

use super::super::{changed, snapshot, MAX_CONFIG_BYTES};
use super::put;

#[test]
fn edits_inside_a_claude_worktree_change_nothing() {
    let root = tempdir().unwrap();
    put(root.path(), ".claude/settings.json", "{}\n");
    put(root.path(), ".claude/worktrees/feat/src/lib.rs", "// a\n");
    put(root.path(), ".claude/worktrees/feat/CLAUDE.md", "# A\n");
    let before = snapshot(root.path()).unwrap();
    put(root.path(), ".claude/worktrees/feat/src/lib.rs", "// b\n");
    put(root.path(), ".claude/worktrees/feat/CLAUDE.md", "# B\n");
    put(root.path(), ".claude/worktrees/feat/.claude/x.json", "{}\n");
    put(root.path(), ".claude/worktrees/other/new.rs", "// new\n");

    let after = snapshot(root.path()).unwrap();

    assert_eq!(changed(&before, &after), Vec::<String>::new());
    let paths: Vec<&String> = after.files.keys().collect();
    assert_eq!(paths, [".claude/settings.json"]);
}

#[test]
fn an_oversized_file_in_a_claude_worktree_does_not_fail_the_snapshot() {
    let root = tempdir().unwrap();
    put(root.path(), ".claude/settings.json", "{}\n");
    put(root.path(), ".claude/worktrees/feat/target/.keep", "");
    let huge = |relative: &str| {
        let file = File::create(root.path().join(relative)).unwrap();
        file.set_len(MAX_CONFIG_BYTES as u64 + 1).unwrap();
    };
    huge(".claude/worktrees/feat/target/huge.bin");

    assert!(snapshot(root.path()).is_ok());

    // The same file anywhere else under `.claude/` still fails closed.
    put(root.path(), ".claude/other/.keep", "");
    huge(".claude/other/huge.bin");
    assert!(snapshot(root.path()).is_err());
}

#[test]
fn a_settings_edit_is_still_detected_beside_claude_worktrees() {
    let root = tempdir().unwrap();
    put(root.path(), ".claude/settings.json", "{\"a\":1}");
    put(root.path(), ".claude/worktrees/feat/src/lib.rs", "// a\n");
    let before = snapshot(root.path()).unwrap();
    put(root.path(), ".claude/settings.json", "{\"a\":2}");
    put(root.path(), ".claude/worktrees/feat/src/lib.rs", "// b\n");

    let after = snapshot(root.path()).unwrap();

    assert_eq!(changed(&before, &after), [".claude/settings.json"]);
}

#[test]
fn a_worktrees_symlink_is_followed_like_any_other_link() {
    let root = tempdir().unwrap();
    put(root.path(), "elsewhere/a.md", "# A\n");
    fs::create_dir(root.path().join(".claude")).unwrap();
    symlink("../elsewhere", root.path().join(".claude/worktrees")).unwrap();
    let before = snapshot(root.path()).unwrap();
    put(root.path(), "elsewhere/a.md", "# A, edited\n");

    let after = snapshot(root.path()).unwrap();

    assert_eq!(changed(&before, &after), [".claude/worktrees/a.md"]);
}

#[test]
fn a_link_serving_a_directory_under_claude_worktrees_is_still_walked() {
    let root = tempdir().unwrap();
    put(root.path(), ".claude/worktrees/feat/src/lib.rs", "// a\n");
    // Sorts before `worktrees`, so it is met before the skipped directory in the listing.
    symlink("worktrees/feat", root.path().join(".claude/a")).unwrap();
    let before = snapshot(root.path()).unwrap();
    put(root.path(), ".claude/worktrees/feat/src/lib.rs", "// b\n");

    let after = snapshot(root.path()).unwrap();

    assert_eq!(changed(&before, &after), [".claude/a/src/lib.rs"]);
    assert!(!after.files.keys().any(|path| path.contains("worktrees/")));
}
