//! Tests for the unattended workspace-repair pass `loom init` runs before
//! `validate_work_dir_state` judges the workspace (see `execute::startup_repairs`).
//!
//! Every function name begins with `init_repair_`: the stage's acceptance
//! runs `cargo test ... init_repair`, and the module path
//! `commands::init::repair_tests::<name>` does not itself contain that
//! string, so the name has to carry it.

use std::fs;
use std::os::unix::fs::symlink;

use serial_test::serial;
use tempfile::TempDir;

use super::execute::{startup_repair_lines, startup_repairs};
use crate::commands::repair::workspace::AppliedRepair;
use crate::fs::permissions::LOOM_PERMISSIONS;
use crate::fs::work_integrity::validate_work_dir_state;
use crate::git::install_pre_commit_hook;

/// `startup_repairs(root, false)` runs `repair_workspace`, whose
/// `HooksAndSettings`/`HookScripts` fixes call the bare
/// `install_loom_hooks()`/`install_codex_hooks()` (see
/// `commands/repair/workspace/tests.rs`'s `HomeGuard`), which read
/// `dirs::home_dir()` unconditionally. None of these fixtures pre-seed a
/// complete `.claude/settings.json` and an up-to-date Codex hook tree, so
/// every caller here trips one or both fixes; redirect `HOME` for the
/// duration so that write lands in a scratch directory instead of the
/// developer's real `~/.claude`/`~/.codex`. `#[serial]` on every caller
/// joins the same default group `fs::permissions::trust`'s HOME-mutating
/// tests use, so none race each other over the shared env var.
fn startup_repairs_isolated(
    root: &std::path::Path,
    no_repair: bool,
) -> anyhow::Result<Vec<AppliedRepair>> {
    let fake_home = TempDir::new().unwrap();
    let original = std::env::var_os("HOME");
    std::env::set_var("HOME", fake_home.path());
    let result = startup_repairs(root, no_repair);
    match original {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
    result
}

/// Write a `.gitignore` complete enough that neither `.loom/work` nor
/// `.worktrees` register as missing.
fn write_complete_gitignore(root: &std::path::Path) {
    fs::write(
        root.join(".gitignore"),
        ".loom/work/\n.loom/work\n.worktrees/\n.worktrees\n",
    )
    .unwrap();
}

/// Write `.claude/settings.json` carrying every `LOOM_PERMISSIONS` entry, so
/// the "Project .claude/settings.json incomplete" check has nothing to raise.
fn write_complete_settings_json(root: &std::path::Path) {
    fs::create_dir_all(root.join(".claude")).unwrap();
    let settings = serde_json::json!({
        "permissions": { "allow": LOOM_PERMISSIONS }
    });
    fs::write(
        root.join(".claude/settings.json"),
        serde_json::to_string_pretty(&settings).unwrap(),
    )
    .unwrap();
}

/// Point `.loom/work` at a symlink to a sibling directory - the shape a
/// committed worktree symlink leaves behind in the main repo.
fn corrupt_work_symlink(root: &std::path::Path, target: &std::path::Path) {
    fs::create_dir_all(root.join(".loom")).unwrap();
    symlink(target, root.join(".loom/work")).unwrap();
}

#[test]
#[serial]
fn init_repair_heals_a_corrupted_work_symlink_before_validation() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let target = temp.path().join("sibling-target");
    fs::create_dir_all(&target).unwrap();
    corrupt_work_symlink(root, &target);

    assert!(
        validate_work_dir_state(root).is_err(),
        "corrupted symlink must fail validation before repair"
    );

    let repairs = startup_repairs_isolated(root, false).unwrap();
    assert!(
        repairs
            .iter()
            .any(|r| r.description.contains("is a symlink (->")),
        "expected a symlink repair, got: {repairs:?}"
    );

    assert!(
        validate_work_dir_state(root).is_ok(),
        "workspace must validate cleanly after repair"
    );
}

/// Contrast with `init_repair_heals_a_corrupted_work_symlink_before_validation`
/// above: that test's root is main-repo-shaped and the symlink there IS
/// corruption, so it must still be healed. Here the root is
/// `.worktrees/<stage>`-shaped, where a `.loom/work` symlink is the correct
/// shape (see `validate_work_dir_state`), and must survive the unattended pass.
#[test]
#[serial]
fn init_repair_leaves_a_worktree_symlink_in_place() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join(".worktrees").join("some-stage");
    fs::create_dir_all(&root).unwrap();
    let target = temp.path().join("sibling-target");
    fs::create_dir_all(&target).unwrap();
    corrupt_work_symlink(&root, &target);

    let repairs = startup_repairs_isolated(&root, false).unwrap();

    assert!(
        !repairs
            .iter()
            .any(|r| r.description.contains("is a symlink (->")),
        "a worktree's work symlink is the correct shape and must not be repaired: {repairs:?}"
    );
    assert!(
        root.join(".loom/work").is_symlink(),
        "the worktree symlink must survive the unattended repair pass"
    );
}

#[test]
#[serial]
fn init_repair_adds_the_missing_gitignore_entries() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    assert!(!root.join(".gitignore").exists());

    let repairs = startup_repairs_isolated(root, false).unwrap();

    let gitignore = fs::read_to_string(root.join(".gitignore")).unwrap();
    assert!(gitignore.contains(".loom/work/"));
    assert!(gitignore.contains(".loom/work"));
    assert!(gitignore.contains(".worktrees/"));
    assert!(gitignore.contains(".worktrees"));

    assert!(
        repairs
            .iter()
            .any(|r| r.description.contains(".loom/work not found in .gitignore")),
        "expected a .loom/work gitignore repair, got: {repairs:?}"
    );
    assert!(
        repairs
            .iter()
            .any(|r| r.description.contains(".worktrees not found in .gitignore")),
        "expected a .worktrees gitignore repair, got: {repairs:?}"
    );
}

#[test]
fn init_repair_skips_every_repair_under_no_repair() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let target = temp.path().join("sibling-target");
    fs::create_dir_all(&target).unwrap();
    corrupt_work_symlink(root, &target);

    let repairs = startup_repairs(root, true).unwrap();

    assert!(repairs.is_empty());
    assert!(root.join(".loom/work").is_symlink());
    assert!(!root.join(".gitignore").exists());
    assert!(validate_work_dir_state(root).is_err());
}

#[test]
#[serial]
fn init_repair_renders_no_line_for_a_clean_workspace() {
    assert!(startup_repair_lines(&[]).is_empty());

    let temp = TempDir::new().unwrap();
    let root = temp.path();
    write_complete_gitignore(root);
    write_complete_settings_json(root);
    fs::create_dir_all(root.join(".git")).unwrap();
    install_pre_commit_hook(root).unwrap();

    // Everything repo-local is now clean. The installed Claude and Codex hook
    // checks read `HOME`, not the scratch repo — `startup_repairs_isolated`
    // points that at a scratch directory too, so they are the only repairs a
    // returned vector may still carry here.
    let repairs = startup_repairs_isolated(root, false).unwrap();
    for repair in &repairs {
        assert!(
            repair.description.contains("Loom hook scripts")
                || repair.description == "Codex hook installation incomplete or outdated",
            "unexpected non-clean repair on an otherwise clean workspace: {repairs:?}"
        );
    }
}

#[test]
#[serial]
fn init_repair_leaves_the_dangerous_fixes_to_an_explicit_repair() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    fs::create_dir_all(root.join(".loom")).unwrap();
    fs::write(root.join(".loom/work"), "not a directory").unwrap();

    let repairs = startup_repairs_isolated(root, false).unwrap();

    assert!(root.join(".loom/work").is_file());
    assert!(
        !repairs
            .iter()
            .any(|r| r.description.contains("exists but is neither")),
        "the invalid-shape fix is a recursive delete and must stay unattended-off: {repairs:?}"
    );
}

#[test]
fn init_repair_renders_one_line_per_applied_repair() {
    let applied = vec![
        AppliedRepair {
            description: "first repair".to_string(),
        },
        AppliedRepair {
            description: "second repair".to_string(),
        },
    ];

    let lines = startup_repair_lines(&applied);

    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains("first repair") && lines[0].contains("Repaired"));
    assert!(lines[1].contains("second repair") && lines[1].contains("Repaired"));
}
