use super::Orchestrator;
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::core::OrchestratorConfig;
use crate::plan::ExecutionGraph;
use serial_test::serial;

/// Run `git` in `root` with ambient global/system config neutralized.
///
/// Mirrors `git::merge::mod::tests::isolated_git`: a global
/// `commit.gpgsign=true` with no configured key (or other ambient config) can
/// break a fresh-repo commit; pinning `GIT_CONFIG_GLOBAL`/`GIT_CONFIG_SYSTEM`
/// to nonexistent paths makes this test depend only on the repo's own local
/// config.
fn isolated_git(root: &std::path::Path, args: &[&str]) -> std::process::Output {
    std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", root.join(".loom-test-no-global"))
        .env("GIT_CONFIG_SYSTEM", root.join(".loom-test-no-system"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap()
}

/// Run a setup `git` command and assert it succeeded, surfacing stderr on
/// failure rather than letting it silently fall through to a confusing
/// assertion several lines down.
fn git_ok(root: &std::path::Path, args: &[&str]) {
    let out = isolated_git(root, args);
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// `Orchestrator::new` eagerly constructs a `NativeBackend`, so it fails on a
/// headless CI runner with no terminal emulator installed. Pinning
/// `LOOM_TERMINAL` maps a name straight to an emulator without probing the
/// host for the binary. Serialized because the detection tests mutate the same
/// process-global variable.
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

#[test]
#[serial]
fn merge_probe_failure_does_not_consume_resolver_attempt_budget() {
    let temp = tempfile::tempdir().unwrap();
    let work_dir = temp.path().join(".loom").join("work");
    let config = OrchestratorConfig {
        work_dir: work_dir.clone(),
        repo_root: temp.path().to_path_buf(),
        enable_skill_routing: false,
        ..Default::default()
    };
    let mut stage = Stage::new("probe-failure".to_string(), None);
    stage.id = "probe-failure".to_string();
    stage.status = StageStatus::MergeConflict;
    crate::verify::transitions::save_stage(&stage, &work_dir).unwrap();
    let saved_terminal = pin_terminal_env();
    let constructed = Orchestrator::new(config, ExecutionGraph::build(Vec::new()).unwrap());
    restore_terminal_env(saved_terminal);
    let mut orchestrator = constructed.unwrap();

    assert_eq!(orchestrator.spawn_merge_resolution_sessions().unwrap(), 0);
    assert_eq!(orchestrator.merge_resolver_attempts(&stage.id), 0);
    assert!(!orchestrator
        .merge_resolver_attempts_dir()
        .join(format!("{}.count", stage.id))
        .exists());
}

/// `try_auto_merge`'s already-merged short circuit (`stage.merged == true`)
/// routes cleanup through `MergeLifecycle::cleanup`, whose containment
/// predicate is the ONLY guard on that path — nothing upstream re-checks
/// ancestry. A stage marked `merged: true` with no `completed_commit` and a
/// branch that still holds commits beyond the target must NOT have its
/// worktree or branch removed: `containment_refusal` cannot prove those
/// commits landed, so cleanup must refuse rather than destroy real work.
/// Build a repo whose `loom/<stage_id>` branch carries a commit that never
/// reached `main`, with its worktree still in place.
///
/// Returns the tempdir — which the caller must keep alive for the duration of
/// the test — and the worktree path.
fn repo_with_unmerged_stage_branch(stage_id: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    git_ok(root, &["init", "-b", "main"]);
    git_ok(root, &["config", "user.email", "t@t.com"]);
    git_ok(root, &["config", "user.name", "t"]);
    std::fs::write(root.join("a.txt"), "seed").unwrap();
    git_ok(root, &["add", "a.txt"]);
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
    // Commit on the stage branch that never made it to `main` — the branch
    // is now provably ahead of the target.
    std::fs::write(worktree_path.join("b.txt"), "unmerged work").unwrap();
    git_ok(&worktree_path, &["add", "b.txt"]);
    git_ok(&worktree_path, &["commit", "-m", "unmerged work"]);

    (temp, worktree_path)
}

#[test]
#[serial]
fn already_merged_short_circuit_refuses_cleanup_for_unmerged_branch() {
    let stage_id = "unmerged-but-flagged";
    let (temp, worktree_path) = repo_with_unmerged_stage_branch(stage_id);
    let root = temp.path();
    let work_dir = root.join(".loom").join("work");
    let branch = format!("loom/{stage_id}");

    let config = OrchestratorConfig {
        work_dir: work_dir.clone(),
        repo_root: root.to_path_buf(),
        base_branch: Some("main".to_string()),
        enable_skill_routing: false,
        ..Default::default()
    };
    let mut stage = Stage::new(stage_id.to_string(), None);
    stage.id = stage_id.to_string();
    stage.status = StageStatus::Completed;
    stage.merged = true;
    crate::verify::transitions::save_stage(&stage, &work_dir).unwrap();

    let saved_terminal = pin_terminal_env();
    let constructed = Orchestrator::new(config, ExecutionGraph::build(Vec::new()).unwrap());
    restore_terminal_env(saved_terminal);
    let mut orchestrator = constructed.unwrap();

    assert!(orchestrator.try_auto_merge(stage_id));

    assert!(
        worktree_path.exists(),
        "cleanup must refuse to remove the worktree: the branch holds commits \
         not provably in 'main'"
    );
    assert!(
        isolated_git(
            root,
            &["rev-parse", "--verify", &format!("refs/heads/{branch}")]
        )
        .status
        .success(),
        "cleanup must refuse to delete the branch: it still holds unmerged commits"
    );
}
