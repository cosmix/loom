//! Tests for git/worktree/settings.rs: env-var scrubbing on create/refresh,
//! and the end-to-end git-based regression for the worktree gitignore fix.

use super::*;
use std::path::Path;
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

#[test]
fn test_refresh_preserves_worktree_env_hooks_and_mode() {
    // Regression test: refresh_worktree_settings_local used the MAIN
    // repo's settings as the merge base, clobbering the worktree's
    // session-specific env, hooks, and resolved defaultMode with whatever
    // the last main-repo session left behind (stale LOOM_STAGE_ID etc.).
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path().join("repo");
    let worktree = temp_dir.path().join("worktree");

    let main_claude = repo_root.join(".claude");
    std::fs::create_dir_all(&main_claude).unwrap();
    let main_settings = json!({
        "permissions": { "allow": ["Read(main_perm)"], "defaultMode": "default" },
        "env": {
            "LOOM_STAGE_ID": "stale-knowledge-stage",
            "LOOM_SESSION_ID": "stale-session"
        },
        "hooks": { "Stop": [{ "matcher": "*", "hooks": [] }] }
    });
    std::fs::write(
        main_claude.join("settings.local.json"),
        serde_json::to_string_pretty(&main_settings).unwrap(),
    )
    .unwrap();

    let wt_claude = worktree.join(".claude");
    std::fs::create_dir_all(&wt_claude).unwrap();
    let wt_settings = json!({
        "permissions": { "allow": ["Write(worktree_perm)"], "defaultMode": "auto" },
        "env": { "LOOM_WORK_DIR": "/repo/.work" },
        "hooks": { "SessionStart": [{ "matcher": "*", "hooks": [] }] }
    });
    std::fs::write(
        wt_claude.join("settings.local.json"),
        serde_json::to_string_pretty(&wt_settings).unwrap(),
    )
    .unwrap();

    assert!(refresh_worktree_settings_local(&worktree, &repo_root).unwrap());

    let merged: Value = serde_json::from_str(
        &std::fs::read_to_string(wt_claude.join("settings.local.json")).unwrap(),
    )
    .unwrap();

    // Permissions are unioned
    let (allow, _) = extract_permissions(&merged);
    assert!(allow.contains(&"Read(main_perm)".to_string()));
    assert!(allow.contains(&"Write(worktree_perm)".to_string()));

    // Worktree-specific settings survive; main's stale env does not leak in
    assert_eq!(merged["permissions"]["defaultMode"], json!("auto"));
    assert_eq!(merged["env"]["LOOM_WORK_DIR"], json!("/repo/.work"));
    let env = merged["env"].as_object().unwrap();
    assert!(!env.contains_key("LOOM_STAGE_ID"));
    assert!(!env.contains_key("LOOM_SESSION_ID"));
    assert!(merged["hooks"]["SessionStart"].is_array());
    assert!(merged["hooks"].get("Stop").is_none());
}

#[test]
fn test_refresh_without_worktree_settings_scrubs_identity_env() {
    // When the worktree has no settings yet, the main copy is used as
    // base — minus any per-session identity env vars.
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path().join("repo");
    let worktree = temp_dir.path().join("worktree");

    let main_claude = repo_root.join(".claude");
    std::fs::create_dir_all(&main_claude).unwrap();
    let main_settings = json!({
        "permissions": { "allow": ["Read(main_perm)"] },
        "env": { "LOOM_STAGE_ID": "stale", "LOOM_SESSION_ID": "stale", "FOO": "keep" }
    });
    std::fs::write(
        main_claude.join("settings.local.json"),
        serde_json::to_string_pretty(&main_settings).unwrap(),
    )
    .unwrap();

    std::fs::create_dir_all(&worktree).unwrap();

    assert!(refresh_worktree_settings_local(&worktree, &repo_root).unwrap());

    let merged: Value = serde_json::from_str(
        &std::fs::read_to_string(worktree.join(".claude/settings.local.json")).unwrap(),
    )
    .unwrap();

    let env = merged["env"].as_object().unwrap();
    assert!(!env.contains_key("LOOM_STAGE_ID"));
    assert!(!env.contains_key("LOOM_SESSION_ID"));
    assert_eq!(env["FOO"], json!("keep"));
}

/// Run git with ambient configuration neutralized, so a developer's
/// global settings (hooks, gpg signing, default branch, aliases) cannot
/// change what these tests exercise. Mirrors
/// `git::cleanup::tests::isolated_git`.
fn isolated_git(root: &Path, args: &[&str]) -> std::process::Output {
    std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", root.join(".loom-test-no-global"))
        .env("GIT_CONFIG_SYSTEM", root.join(".loom-test-no-system"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap()
}

/// Assert a setup command succeeded. Dropping the exit status here would
/// hide setup failures and turn them into a confusing assertion further
/// down.
fn git_ok(root: &Path, args: &[&str]) {
    let out = isolated_git(root, args);
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
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
    git_ok(&repo_root, &["init"]);
    git_ok(&repo_root, &["config", "user.email", "test@test.com"]);
    git_ok(&repo_root, &["config", "user.name", "Test"]);
    std::fs::write(repo_root.join("README.md"), "# Test").unwrap();
    git_ok(&repo_root, &["add", "README.md"]);
    git_ok(&repo_root, &["commit", "-m", "initial"]);

    let worktrees_dir = repo_root.join(".worktrees");
    std::fs::create_dir_all(&worktrees_dir).unwrap();
    git_ok(
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

    let check_ignore = isolated_git(
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

    let status = isolated_git(&worktree, &["status", "--porcelain"]);
    assert!(status.status.success(), "git status failed to run");
    assert!(
        String::from_utf8_lossy(&status.stdout).trim().is_empty(),
        "worktree should read as clean once loom's runtime paths are excluded, got: {}",
        String::from_utf8_lossy(&status.stdout)
    );
}
