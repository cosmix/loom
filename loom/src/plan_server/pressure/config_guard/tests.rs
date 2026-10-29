use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;

use tempfile::tempdir;

use super::{changed, snapshot, MAX_CONFIG_BYTES};

fn put(root: &Path, relative: &str, bytes: impl AsRef<[u8]>) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

#[test]
fn added_root_config_is_detected() {
    let root = tempdir().unwrap();
    let before = snapshot(root.path()).unwrap();
    put(root.path(), ".mcp.json", "{}\n");
    put(root.path(), "CLAUDE.md", "# Instructions\n");

    let after = snapshot(root.path()).unwrap();

    assert_eq!(changed(&before, &after), [".mcp.json", "CLAUDE.md"]);
}

#[test]
fn modified_file_is_detected_even_with_the_same_length() {
    let root = tempdir().unwrap();
    put(root.path(), ".claude/settings.json", "{\"a\":1}");
    let before = snapshot(root.path()).unwrap();
    put(root.path(), ".claude/settings.json", "{\"a\":2}");

    let after = snapshot(root.path()).unwrap();

    assert_eq!(changed(&before, &after), [".claude/settings.json"]);
}

#[test]
fn removed_file_is_detected() {
    let root = tempdir().unwrap();
    put(root.path(), "CLAUDE.md", "# Instructions\n");
    let before = snapshot(root.path()).unwrap();
    fs::remove_file(root.path().join("CLAUDE.md")).unwrap();

    let after = snapshot(root.path()).unwrap();

    assert_eq!(changed(&before, &after), ["CLAUDE.md"]);
}

#[test]
fn untracked_agent_and_ignored_local_settings_are_detected() {
    let root = tempdir().unwrap();
    put(root.path(), ".claude/settings.json", "{}\n");
    put(root.path(), ".gitignore", ".claude/settings.local.json\n");
    let before = snapshot(root.path()).unwrap();
    put(root.path(), ".claude/settings.local.json", "{}\n");
    put(root.path(), ".claude/agents/x.md", "# Agent\n");

    let after = snapshot(root.path()).unwrap();

    assert_eq!(
        changed(&before, &after),
        [".claude/agents/x.md", ".claude/settings.local.json"]
    );
}

#[test]
fn nested_configuration_is_detected_in_sorted_order() {
    let root = tempdir().unwrap();
    let before = snapshot(root.path()).unwrap();
    put(root.path(), ".claude/z/deep/rules.md", "# Rules\n");
    put(
        root.path(),
        ".claude/a/skills/review/SKILL.md",
        "# Review\n",
    );
    fs::create_dir_all(root.path().join(".claude/empty")).unwrap();

    let after = snapshot(root.path()).unwrap();

    assert_eq!(
        changed(&before, &after),
        [
            ".claude/a/skills/review/SKILL.md",
            ".claude/z/deep/rules.md"
        ]
    );
}

#[test]
fn doc_edit_is_ignored() {
    let root = tempdir().unwrap();
    put(root.path(), ".claude/settings.json", "{}\n");
    put(root.path(), "doc/a.md", "# A\n");
    let before = snapshot(root.path()).unwrap();
    put(root.path(), "doc/a.md", "# A, edited\n");

    let after = snapshot(root.path()).unwrap();

    assert_eq!(changed(&before, &after), Vec::<String>::new());
}

#[test]
fn file_and_directory_symlinks_are_not_followed() {
    let root = tempdir().unwrap();
    let outside = tempdir().unwrap();
    put(outside.path(), "settings.json", "{}\n");
    put(outside.path(), "agents/x.md", "# Agent\n");
    fs::create_dir(root.path().join(".claude")).unwrap();
    let before = snapshot(root.path()).unwrap();
    symlink(outside.path(), root.path().join(".claude/linked")).unwrap();
    symlink(
        outside.path().join("settings.json"),
        root.path().join(".claude/settings.json"),
    )
    .unwrap();
    symlink(
        outside.path().join("missing"),
        root.path().join(".claude/dangling"),
    )
    .unwrap();

    let after = snapshot(root.path()).unwrap();

    assert_eq!(before, after);
}

#[test]
fn root_configuration_symlinks_are_not_followed() {
    let root = tempdir().unwrap();
    let outside = tempdir().unwrap();
    put(outside.path(), "settings.json", "{}\n");
    let before = snapshot(root.path()).unwrap();
    symlink(outside.path(), root.path().join(".claude")).unwrap();
    symlink(
        outside.path().join("settings.json"),
        root.path().join(".mcp.json"),
    )
    .unwrap();
    symlink(
        outside.path().join("settings.json"),
        root.path().join("CLAUDE.md"),
    )
    .unwrap();

    let after = snapshot(root.path()).unwrap();

    assert_eq!(before, after);
}

#[test]
fn binary_configuration_bytes_are_hashed() {
    let root = tempdir().unwrap();
    put(root.path(), ".claude/asset.bin", [0xff, 0, 1]);
    let before = snapshot(root.path()).unwrap();
    put(root.path(), ".claude/asset.bin", [0xff, 0, 2]);

    let after = snapshot(root.path()).unwrap();

    assert_eq!(changed(&before, &after), [".claude/asset.bin"]);
}

#[test]
fn mixed_changes_are_sorted_and_modified_paths_are_not_duplicated() {
    let root = tempdir().unwrap();
    put(root.path(), ".claude/z.md", "removed");
    put(root.path(), ".mcp.json", "old");
    let before = snapshot(root.path()).unwrap();
    fs::remove_file(root.path().join(".claude/z.md")).unwrap();
    put(root.path(), ".claude/a.md", "added");
    put(root.path(), ".mcp.json", "new");

    let after = snapshot(root.path()).unwrap();

    assert_eq!(
        changed(&before, &after),
        [".claude/a.md", ".claude/z.md", ".mcp.json"]
    );
}

#[test]
fn oversized_configuration_fails_instead_of_hashing_a_prefix() {
    let root = tempdir().unwrap();
    put(root.path(), ".mcp.json", vec![b'x'; MAX_CONFIG_BYTES + 1]);

    let error = snapshot(root.path()).unwrap_err();

    assert!(error
        .downcast_ref::<crate::fs::safe_read::OverLimit>()
        .is_some());
}
