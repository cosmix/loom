//! Tests for `trust`: entries written, preserved and removed, and the atomic mode-keeping replace.

use super::*;
use crate::fs::permissions::scratch_home::ScratchHome;
use serial_test::serial;
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;

fn with_test_home(test_fn: impl FnOnce(&Path, &Path)) {
    let home = ScratchHome::set();
    let worktree = TempDir::new().unwrap();

    test_fn(home.path(), worktree.path());
}

#[test]
#[serial]
fn test_trust_worktree_creates_entry() {
    with_test_home(|home, worktree| {
        trust_worktree(worktree).unwrap();

        let state_path = home.join(".claude.json");
        let content = fs::read_to_string(&state_path).unwrap();
        let settings: Value = serde_json::from_str(&content).unwrap();

        let path_str = worktree
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert_eq!(
            settings["projects"][&path_str]["hasTrustDialogAccepted"],
            true
        );
    });
}

#[test]
#[serial]
fn test_trust_worktree_idempotent() {
    with_test_home(|home, worktree| {
        trust_worktree(worktree).unwrap();
        trust_worktree(worktree).unwrap();

        let state_path = home.join(".claude.json");
        let content = fs::read_to_string(&state_path).unwrap();
        let settings: Value = serde_json::from_str(&content).unwrap();

        let projects = settings["projects"].as_object().unwrap();
        assert_eq!(projects.len(), 1);
    });
}

#[test]
#[serial]
fn test_untrust_worktree_removes_entry() {
    with_test_home(|home, worktree| {
        trust_worktree(worktree).unwrap();
        untrust_worktree(worktree).unwrap();

        let state_path = home.join(".claude.json");
        let content = fs::read_to_string(&state_path).unwrap();
        let settings: Value = serde_json::from_str(&content).unwrap();

        let projects = settings["projects"].as_object().unwrap();
        assert!(projects.is_empty());
    });
}

#[test]
#[serial]
fn test_untrust_worktree_noop_if_missing() {
    with_test_home(|_home, worktree| {
        // Should not error even if no state file exists
        untrust_worktree(worktree).unwrap();
    });
}

#[test]
#[serial]
fn test_trust_preserves_existing_data() {
    with_test_home(|home, worktree| {
        let state_path = home.join(".claude.json");
        let existing = json!({
            "projects": {
                "/some/other/project": {
                    "hasTrustDialogAccepted": true,
                    "allowedTools": ["Bash"]
                }
            },
            "otherField": 42
        });
        fs::write(
            &state_path,
            serde_json::to_string_pretty(&existing).unwrap(),
        )
        .unwrap();

        trust_worktree(worktree).unwrap();

        let content = fs::read_to_string(&state_path).unwrap();
        let settings: Value = serde_json::from_str(&content).unwrap();

        // Existing project preserved
        assert_eq!(
            settings["projects"]["/some/other/project"]["hasTrustDialogAccepted"],
            true
        );
        assert_eq!(
            settings["projects"]["/some/other/project"]["allowedTools"][0],
            "Bash"
        );
        // Other fields preserved
        assert_eq!(settings["otherField"], 42);
        // New entry added
        let path_str = worktree
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert_eq!(
            settings["projects"][&path_str]["hasTrustDialogAccepted"],
            true
        );
    });
}

#[test]
#[serial]
fn test_trust_project_writes_only_the_given_home() {
    let scratch = ScratchHome::set();
    let given = TempDir::new().unwrap();
    let project = TempDir::new().unwrap();
    let state_path = given.path().join(".claude.json");
    let existing = json!({
        "projects": {"/some/other/project": {"hasTrustDialogAccepted": true}},
        "otherField": 42
    });
    fs::write(
        &state_path,
        serde_json::to_string_pretty(&existing).unwrap(),
    )
    .unwrap();

    trust_project(given.path(), project.path()).unwrap();

    let settings: Value = serde_json::from_str(&fs::read_to_string(&state_path).unwrap()).unwrap();
    let canonical = project.path().canonicalize().unwrap();
    let key = canonical.to_string_lossy();
    assert_eq!(
        settings["projects"][key.as_ref()]["hasTrustDialogAccepted"],
        true
    );
    assert_eq!(
        settings["projects"]["/some/other/project"]["hasTrustDialogAccepted"],
        true
    );
    assert_eq!(settings["otherField"], 42);

    let written = fs::read(&state_path).unwrap();
    trust_project(given.path(), project.path()).unwrap();
    assert_eq!(fs::read(&state_path).unwrap(), written);
    assert!(!scratch.path().join(".claude.json").exists());
    assert!(trust_project(Path::new(""), project.path()).is_err());
}

#[test]
#[serial]
fn test_trust_project_keeps_the_file_mode_and_leaves_no_temp_file() {
    let _scratch = ScratchHome::set();
    let project = TempDir::new().unwrap();
    // 0664 is what the replace must copy; a temp file's own 0600 would pass the 0600 case alone.
    for mode in [0o600, 0o664] {
        let home = TempDir::new().unwrap();
        let state_path = home.path().join(".claude.json");
        fs::write(&state_path, "{\"otherField\": 42}").unwrap();
        fs::set_permissions(&state_path, fs::Permissions::from_mode(mode)).unwrap();

        trust_project(home.path(), project.path()).unwrap();

        let actual = fs::metadata(&state_path).unwrap().permissions().mode() & 0o777;
        assert_eq!(actual, mode, "mode {actual:o} after the replace");
        let settings: Value =
            serde_json::from_str(&fs::read_to_string(&state_path).unwrap()).unwrap();
        assert_eq!(settings["otherField"], 42);
        let names: Vec<_> = fs::read_dir(home.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, [".claude.json"], "a temp file was left behind");
    }
}

#[test]
#[serial]
fn test_trust_project_writes_through_a_symlinked_state_file() {
    let _scratch = ScratchHome::set();
    let home = TempDir::new().unwrap();
    let dotfiles = TempDir::new().unwrap();
    let project = TempDir::new().unwrap();
    let real = dotfiles.path().join("claude.json");
    fs::write(&real, "{\"otherField\": 42}").unwrap();
    let link = home.path().join(".claude.json");
    std::os::unix::fs::symlink(&real, &link).unwrap();

    trust_project(home.path(), project.path()).unwrap();

    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink(),
        "the symlink was replaced by a file",
    );
    let settings: Value = serde_json::from_str(&fs::read_to_string(&real).unwrap()).unwrap();
    assert_eq!(settings["otherField"], 42);
    let canonical = project.path().canonicalize().unwrap();
    let key = canonical.to_string_lossy();
    assert_eq!(
        settings["projects"][key.as_ref()]["hasTrustDialogAccepted"],
        true
    );
}

#[test]
#[serial]
fn test_migrate_legacy_trust() {
    with_test_home(|home, _worktree| {
        let repo = TempDir::new().unwrap();
        let worktrees_dir = repo.path().join(".worktrees");
        fs::create_dir_all(&worktrees_dir).unwrap();

        let worktrees_path = worktrees_dir
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();

        let state_path = home.join(".claude.json");
        let existing = json!({
            "trustedDirectories": [worktrees_path, "/other/dir"],
            "projects": {}
        });
        fs::write(
            &state_path,
            serde_json::to_string_pretty(&existing).unwrap(),
        )
        .unwrap();

        migrate_legacy_trust(repo.path()).unwrap();

        let content = fs::read_to_string(&state_path).unwrap();
        let settings: Value = serde_json::from_str(&content).unwrap();

        let trusted = settings["trustedDirectories"].as_array().unwrap();
        assert_eq!(trusted.len(), 1);
        assert_eq!(trusted[0], "/other/dir");
    });
}
