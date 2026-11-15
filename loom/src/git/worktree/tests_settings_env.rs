//! Tests for git/worktree/settings.rs: env-var scrubbing on create/refresh,
//! and the end-to-end git-based regression for the worktree gitignore fix.

use super::*;
use crate::test_git::{git, git_unchecked, TestRepo};
use tempfile::TempDir;

#[test]
fn test_create_worktree_settings_preserves_env_for_native() {
    let temp_dir = TempDir::new().unwrap();
    let worktree = temp_dir.path().join("worktree");
    std::fs::create_dir_all(&worktree).unwrap();

    let main_claude = temp_dir.path().join("repo").join(".claude");
    std::fs::create_dir_all(&main_claude).unwrap();
    let main_settings_json = json!({
        "env": {
            "AWS_ACCESS_KEY_ID": "keep-on-native",
            "GH_TOKEN": "keep-on-native",
            "LOOM_MAIN_AGENT_PID": "stale",
            "LOOM_STAGE_ID": "stale-stage",
            "LOOM_SESSION_ID": "stale-session"
        }
    });
    let main_settings_path = main_claude.join("settings.json");
    std::fs::write(
        &main_settings_path,
        serde_json::to_string_pretty(&main_settings_json).unwrap(),
    )
    .unwrap();

    let worktree_settings_path = worktree.join("settings.json");
    create_worktree_settings(&main_settings_path, &worktree_settings_path, &worktree).unwrap();

    let content = std::fs::read_to_string(&worktree_settings_path).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();

    let env = settings["env"].as_object().unwrap();
    assert!(env.contains_key("AWS_ACCESS_KEY_ID"));
    assert!(env.contains_key("GH_TOKEN"));
    assert!(
        !env.contains_key("LOOM_MAIN_AGENT_PID"),
        "LOOM_MAIN_AGENT_PID is always stripped, even on native"
    );
    assert!(
        !env.contains_key("LOOM_STAGE_ID") && !env.contains_key("LOOM_SESSION_ID"),
        "per-session identity env vars must be stripped from inherited settings"
    );
}

/// End-to-end regression for defect 2: writing the excludes to
/// `.git/worktrees/<stage-id>/info/exclude` (the previous behavior)
/// never actually hid anything from git, because git resolves
/// `info/exclude` to the COMMON git dir for every worktree. This test
/// proves the fix by asserting git's own behavior, not just file
/// content: a real worktree's memory spool and cache directory read as
/// ignored (`git check-ignore`) and the worktree reads as clean
/// (`git status --porcelain`) once `add_settings_local_to_worktree_gitignore`
/// has run against it.
#[test]
fn test_worktree_gitignore_actually_hides_loom_runtime_paths() {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path().join("repo");
    std::fs::create_dir_all(&repo_root).unwrap();
    TestRepo::builder()
        .seed("README.md", "# Test", "initial")
        .init_in(&repo_root);

    let worktrees_dir = repo_root.join(".worktrees");
    std::fs::create_dir_all(&worktrees_dir).unwrap();
    git(
        &repo_root,
        &[
            "worktree",
            "add",
            ".worktrees/stage-1",
            "-b",
            "loom/stage-1",
        ],
    );
    let worktree = worktrees_dir.join("stage-1");

    // Exercise the real production call: no stage-id, common git dir.
    add_settings_local_to_worktree_gitignore(&repo_root).unwrap();

    // Populate the same runtime paths a real stage would leave behind.
    std::fs::create_dir_all(worktree.join(".loom/cache/context-v1")).unwrap();
    std::fs::write(worktree.join(".loom/cache/context-v1/catalog.json"), "{}").unwrap();
    std::fs::write(worktree.join(".loom/memory-spool.jsonl"), "").unwrap();

    let check_ignore = git_unchecked(
        &worktree,
        &[
            "check-ignore",
            "-v",
            ".loom/memory-spool.jsonl",
            ".loom/cache/context-v1/catalog.json",
        ],
    );
    assert!(
        check_ignore.status.success(),
        "git should report both loom runtime paths as ignored, stdout: {}, stderr: {}",
        String::from_utf8_lossy(&check_ignore.stdout),
        String::from_utf8_lossy(&check_ignore.stderr)
    );

    let status = git_unchecked(&worktree, &["status", "--porcelain"]);
    assert!(status.status.success(), "git status failed to run");
    assert!(
        String::from_utf8_lossy(&status.stdout).trim().is_empty(),
        "worktree should read as clean once loom's runtime paths are excluded, got: {}",
        String::from_utf8_lossy(&status.stdout)
    );
}
