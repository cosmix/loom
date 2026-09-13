//! Git-backed acceptance tests for the merge gate (owner decision 9), plus
//! pure unit tests for its path logic.

use super::*;
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::core::OrchestratorConfig;
use crate::plan::ExecutionGraph;
use serial_test::serial;
use std::path::PathBuf;

/// Run `git` in `root` with ambient global/system config neutralized (mirrors
/// `merge_handler_attempt_tests::isolated_git`).
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

fn git_ok(root: &Path, args: &[&str]) {
    let out = isolated_git(root, args);
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Pin `LOOM_TERMINAL` so `Orchestrator::new` (which eagerly constructs a
/// `NativeBackend`) does not probe a headless test runner for an emulator.
/// Mirrors `merge_handler_attempt_tests::pin_terminal_env`; callers must be
/// `#[serial]`, since this mutates a process-global variable.
fn pin_terminal_env() -> Option<std::ffi::OsString> {
    let saved = std::env::var_os("LOOM_TERMINAL");
    // SAFETY: the test is serialized and restores the original value below.
    unsafe { std::env::set_var("LOOM_TERMINAL", "xterm") };
    saved
}

fn restore_terminal_env(saved: Option<std::ffi::OsString>) {
    match saved {
        // SAFETY: the serialized test is restoring its saved value.
        Some(value) => unsafe { std::env::set_var("LOOM_TERMINAL", value) },
        // SAFETY: the serialized test is restoring the variable's absence.
        None => unsafe { std::env::remove_var("LOOM_TERMINAL") },
    }
}

fn orchestrator_for(root: &Path, work_dir: &Path) -> Orchestrator {
    let config = OrchestratorConfig {
        work_dir: work_dir.to_path_buf(),
        repo_root: root.to_path_buf(),
        base_branch: Some("main".to_string()),
        enable_skill_routing: false,
        ..Default::default()
    };
    let saved_terminal = pin_terminal_env();
    let constructed = Orchestrator::new(config, ExecutionGraph::build(Vec::new()).unwrap());
    restore_terminal_env(saved_terminal);
    constructed.unwrap()
}

fn save_completed_unmerged_stage(stage_id: &str, work_dir: &Path) {
    let mut stage = Stage::new(stage_id.to_string(), None);
    stage.id = stage_id.to_string();
    stage.status = StageStatus::Completed;
    stage.merged = false;
    stage.completed_commit = None;
    crate::verify::transitions::save_stage(&stage, work_dir).unwrap();
}

/// A repo with `main` at a seed commit and a `loom/<stage_id>` branch, built
/// in its own worktree, one commit ahead writing every `(path, content)` in
/// `touch`. Returns the tempdir (kept alive for the test's duration) and the
/// worktree path.
fn repo_with_stage_branch(stage_id: &str, touch: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    git_ok(root, &["init", "-b", "main"]);
    git_ok(root, &["config", "user.email", "t@t.com"]);
    git_ok(root, &["config", "user.name", "t"]);
    std::fs::write(root.join("seed.txt"), "seed").unwrap();
    git_ok(root, &["add", "seed.txt"]);
    git_ok(root, &["commit", "-m", "seed"]);

    let worktree_path = root.join(".worktrees").join(stage_id);
    let branch = format!("loom/{stage_id}");
    git_ok(
        root,
        &[
            "worktree",
            "add",
            "-b",
            &branch,
            worktree_path.to_str().unwrap(),
        ],
    );
    for (path, content) in touch {
        let file_path = worktree_path.join(path);
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&file_path, content).unwrap();
        git_ok(&worktree_path, &["add", path]);
    }
    git_ok(&worktree_path, &["commit", "-m", "stage work"]);

    (temp, worktree_path)
}

/// [`repo_with_stage_branch`], plus a `main`-side commit that conflicts with
/// the stage branch's `shared.txt` (an add/add conflict), so a real merge
/// attempt would need conflict resolution.
fn repo_with_conflicting_stage_branch(stage_id: &str) -> (tempfile::TempDir, PathBuf) {
    let (temp, worktree_path) = repo_with_stage_branch(
        stage_id,
        &[
            ("shared.txt", "stage change\n"),
            (".claude/settings.json", "{}\n"),
        ],
    );
    let root = temp.path();
    std::fs::write(root.join("shared.txt"), "main change\n").unwrap();
    git_ok(root, &["add", "shared.txt"]);
    git_ok(root, &["commit", "-m", "main diverges"]);
    (temp, worktree_path)
}

fn assert_held(root: &Path, work_dir: &Path, stage_id: &str, offending_path: &str) {
    let mut orchestrator = orchestrator_for(root, work_dir);
    assert!(
        !orchestrator.try_auto_merge(stage_id),
        "a control-path-touching branch must not merge"
    );
    let reloaded = crate::verify::transitions::load_stage(stage_id, work_dir).unwrap();
    assert_eq!(reloaded.status, StageStatus::NeedsHumanReview);
    assert!(!reloaded.merged);
    let reason = reloaded
        .review_reason
        .expect("routing to human review must record review_reason");
    assert!(
        reason.contains(offending_path),
        "review_reason must name the offending path {offending_path}: {reason}"
    );
    assert!(
        orchestrator.active_sessions.is_empty(),
        "a held stage must never get a merge-resolution session"
    );
}

#[test]
#[serial]
fn a_branch_touching_claude_settings_is_held() {
    let stage_id = "touches-claude";
    let (temp, _worktree) = repo_with_stage_branch(stage_id, &[(".claude/settings.json", "{}\n")]);
    let root = temp.path();
    let work_dir = root.join(".loom").join("work");
    save_completed_unmerged_stage(stage_id, &work_dir);

    assert_held(root, &work_dir, stage_id, ".claude/settings.json");
}

#[test]
#[serial]
fn a_branch_touching_mcp_json_is_held() {
    let stage_id = "touches-mcp";
    let (temp, _worktree) = repo_with_stage_branch(stage_id, &[(".mcp.json", "{}\n")]);
    let root = temp.path();
    let work_dir = root.join(".loom").join("work");
    save_completed_unmerged_stage(stage_id, &work_dir);

    assert_held(root, &work_dir, stage_id, ".mcp.json");
}

#[test]
#[serial]
fn a_branch_touching_dot_loom_is_held() {
    let stage_id = "touches-dot-loom";
    let (temp, _worktree) = repo_with_stage_branch(stage_id, &[(".loom/x", "x\n")]);
    let root = temp.path();
    let work_dir = root.join(".loom").join("work");
    save_completed_unmerged_stage(stage_id, &work_dir);

    assert_held(root, &work_dir, stage_id, ".loom/x");
}

#[test]
#[serial]
fn a_branch_touching_the_configured_hooks_dir_is_held() {
    let stage_id = "touches-hooks";
    let (temp, _worktree) =
        repo_with_stage_branch(stage_id, &[("loom/.githooks/pre-commit", "#!/bin/sh\n")]);
    let root = temp.path();
    git_ok(root, &["config", "core.hooksPath", "loom/.githooks"]);
    let work_dir = root.join(".loom").join("work");
    save_completed_unmerged_stage(stage_id, &work_dir);

    assert_held(root, &work_dir, stage_id, "loom/.githooks/pre-commit");
}

#[test]
#[serial]
fn a_branch_touching_only_ordinary_source_merges_as_before() {
    let stage_id = "ordinary-source";
    let (temp, _worktree) = repo_with_stage_branch(stage_id, &[("src/x.rs", "fn x() {}\n")]);
    let root = temp.path();
    let work_dir = root.join(".loom").join("work");
    save_completed_unmerged_stage(stage_id, &work_dir);

    let mut orchestrator = orchestrator_for(root, &work_dir);
    assert!(
        orchestrator.try_auto_merge(stage_id),
        "a branch touching no control path must merge exactly as before"
    );
    let reloaded = crate::verify::transitions::load_stage(stage_id, &work_dir).unwrap();
    assert!(reloaded.merged);
    assert!(isolated_git(root, &["show", "main:src/x.rs"])
        .status
        .success());
}

#[test]
#[serial]
fn a_conflicting_branch_touching_claude_settings_spawns_no_resolver() {
    let stage_id = "conflicting-and-touches-claude";
    let (temp, _worktree) = repo_with_conflicting_stage_branch(stage_id);
    let root = temp.path();
    let work_dir = root.join(".loom").join("work");
    save_completed_unmerged_stage(stage_id, &work_dir);

    assert_held(root, &work_dir, stage_id, ".claude/settings.json");

    let mut orchestrator = orchestrator_for(root, &work_dir);
    assert_eq!(
        orchestrator.spawn_merge_resolution_sessions().unwrap(),
        0,
        "a held stage is no longer MergeConflict/MergeBlocked, so the resolver \
         spawn loop must not touch it"
    );
}

#[test]
fn control_path_violation_names_every_offending_path() {
    let (temp, _worktree) = repo_with_stage_branch(
        "multi",
        &[
            (".claude/settings.json", "{}\n"),
            ("src/ok.rs", "fn ok() {}\n"),
        ],
    );
    let root = temp.path();

    let reason = control_path_violation(root, "main", "loom/multi")
        .unwrap()
        .expect("a control path was touched");
    assert!(reason.contains(".claude/settings.json"));
    assert!(!reason.contains("src/ok.rs"));
}

#[test]
fn control_path_violation_is_none_for_an_ordinary_branch() {
    let (temp, _worktree) = repo_with_stage_branch("clean", &[("src/ok.rs", "fn ok() {}\n")]);
    let root = temp.path();

    assert_eq!(
        control_path_violation(root, "main", "loom/clean").unwrap(),
        None
    );
}

#[test]
fn is_control_path_matches_the_documented_prefixes() {
    assert!(is_control_path(".claude/settings.json", None));
    assert!(is_control_path(".mcp.json", None));
    assert!(is_control_path(".loom/cache/x", None));
    assert!(!is_control_path("src/x.rs", None));
    assert!(!is_control_path("claude/x", None));
    assert!(is_control_path(
        "loom/.githooks/pre-commit",
        Some("loom/.githooks/")
    ));
    assert!(!is_control_path("loom/.githooks/pre-commit", None));
}

#[test]
fn hooks_dir_prefix_reads_a_relative_core_hooks_path() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    git_ok(root, &["init", "-b", "main"]);
    git_ok(root, &["config", "core.hooksPath", "loom/.githooks"]);

    assert_eq!(hooks_dir_prefix(root), Some("loom/.githooks/".to_string()));
}

#[test]
fn hooks_dir_prefix_is_none_when_unset() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    git_ok(root, &["init", "-b", "main"]);

    assert_eq!(hooks_dir_prefix(root), None);
}

#[test]
fn resolve_hooks_dir_prefix_local_beats_global() {
    let root = Path::new("/repo");
    assert_eq!(
        resolve_hooks_dir_prefix(root, Some("local/hooks"), Some("global/hooks"), None),
        Some("local/hooks/".to_string())
    );
}

#[test]
fn resolve_hooks_dir_prefix_uses_a_relative_global_when_local_is_unset() {
    let root = Path::new("/repo");
    assert_eq!(
        resolve_hooks_dir_prefix(root, None, Some(".githooks"), None),
        Some(".githooks/".to_string())
    );
}

#[test]
fn resolve_hooks_dir_prefix_falls_back_to_system_when_local_and_global_are_unset() {
    let root = Path::new("/repo");
    assert_eq!(
        resolve_hooks_dir_prefix(root, None, None, Some("system/hooks")),
        Some("system/hooks/".to_string())
    );
}

#[test]
fn resolve_hooks_dir_prefix_is_none_for_an_absolute_path_outside_the_repo() {
    let root = Path::new("/repo");
    assert_eq!(
        resolve_hooks_dir_prefix(root, Some("/elsewhere/hooks"), None, None),
        None
    );
}

#[test]
fn resolve_hooks_dir_prefix_resolves_an_absolute_path_inside_the_repo() {
    let root = Path::new("/repo");
    assert_eq!(
        resolve_hooks_dir_prefix(root, Some("/repo/loom/.githooks"), None, None),
        Some("loom/.githooks/".to_string())
    );
}

#[test]
fn resolve_hooks_dir_prefix_is_none_when_every_scope_is_unset() {
    let root = Path::new("/repo");
    assert_eq!(resolve_hooks_dir_prefix(root, None, None, None), None);
}
