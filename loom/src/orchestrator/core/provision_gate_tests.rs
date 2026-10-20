//! Tests for [`Orchestrator::pre_spawn_gates_passed`]: the before-stage checks run
//! first, then the snapshot's provision entries; a failure of either blocks the stage.

use super::*;
use crate::fs::work_dir::write_terminal_config;
use crate::models::failure::FailureType;
use crate::models::session::{SessionBackendKind, TerminalConfig};
use crate::models::stage::TruthCheck;
use crate::orchestrator::core::OrchestratorConfig;
use crate::orchestrator::provision::write_provision_snapshot;
use crate::plan::ExecutionGraph;
use crate::verify::transitions::{load_stage, save_stage};
use std::fs;
use std::process::Command;
use tempfile::TempDir;

/// A `.loom/work` directory whose configured terminal lane is tmux, so
/// `Orchestrator::new` never runs real terminal detection (which fails on a
/// headless test runner). The provision snapshot is merged into this same
/// `config.toml`, never written over it.
fn work_dir() -> TempDir {
    let temp = TempDir::new().unwrap();
    let work = temp.path().join(".loom").join("work");
    fs::create_dir_all(&work).unwrap();
    write_terminal_config(
        &work,
        &TerminalConfig {
            backend: SessionBackendKind::Tmux,
        },
    )
    .unwrap();
    temp
}

fn orchestrator_for(work_dir: &Path, repo_root: &Path) -> Orchestrator {
    let config = OrchestratorConfig {
        work_dir: work_dir.to_path_buf(),
        repo_root: repo_root.to_path_buf(),
        enable_skill_routing: false,
        ..Default::default()
    };
    Orchestrator::new(config, ExecutionGraph::build(Vec::new()).unwrap()).unwrap()
}

fn entry(command: &str) -> ProvisionEntry {
    ProvisionEntry {
        working_dir: ".".to_string(),
        command: command.to_string(),
    }
}

/// A Queued stage `alpha`, saved under `work`.
fn queued_stage(work: &Path, before_stage: Vec<TruthCheck>) -> Stage {
    let mut stage = Stage::new("alpha".to_string(), None);
    stage.id = "alpha".to_string();
    stage.status = StageStatus::Queued;
    stage.before_stage = before_stage;
    save_stage(&stage, work).unwrap();
    stage
}

/// Run the gate for a Queued `alpha` whose snapshot holds `commands`, with
/// `worktree` as the stage worktree. Returns the gate's answer and the stage
/// as the gate left it on disk.
fn gate(commands: &[&str], before_stage: Vec<TruthCheck>, worktree: &Path) -> (bool, Stage) {
    let temp = work_dir();
    let work = temp.path().join(".loom").join("work");
    let entries: Vec<ProvisionEntry> = commands.iter().map(|c| entry(c)).collect();
    write_provision_snapshot(&work, &entries).unwrap();
    let stage = queued_stage(&work, before_stage);
    let mut orchestrator = orchestrator_for(&work, temp.path());

    let passed = orchestrator
        .pre_spawn_gates_passed(&stage, worktree, "main")
        .unwrap();

    (passed, load_stage("alpha", &work).unwrap())
}

fn run_git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", root.join(".loom-test-no-global"))
        .env("GIT_CONFIG_SYSTEM", root.join(".loom-test-no-system"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A git repository committing a `.gitignore` that holds `node_modules/`.
fn repo_ignoring_node_modules() -> TempDir {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    run_git(root, &["init", "-q"]);
    run_git(root, &["config", "user.name", "t"]);
    run_git(root, &["config", "user.email", "t@t"]);
    fs::write(root.join(".gitignore"), "node_modules/\n").unwrap();
    run_git(root, &["add", ".gitignore"]);
    run_git(root, &["commit", "-q", "-m", "seed"]);
    temp
}

/// [`repo_ignoring_node_modules`] that also commits `path` with the content `seed`.
fn repo_tracking(path: &str) -> TempDir {
    let repo = repo_ignoring_node_modules();
    let file = repo.path().join(path);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, "seed\n").unwrap();
    run_git(repo.path(), &["add", path]);
    run_git(repo.path(), &["commit", "-q", "-m", "track"]);
    repo
}

fn failing_check() -> TruthCheck {
    TruthCheck {
        command: "false".to_string(),
        stdout_contains: Vec::new(),
        stdout_not_contains: Vec::new(),
        stderr_empty: None,
        exit_code: Some(0),
        timeout_secs: None,
        description: Some("the pre-condition holds".to_string()),
    }
}

#[test]
fn a_failing_provision_blocks_the_stage_with_its_reason() {
    let worktree = TempDir::new().unwrap();

    let (passed, stage) = gate(&["exit 3"], Vec::new(), worktree.path());

    assert!(!passed);
    assert_eq!(stage.status, StageStatus::Blocked);
    let reason = stage.close_reason.expect("the block records its reason");
    assert!(
        reason.starts_with("provision `exit 3` in `.` failed"),
        "{reason}"
    );
    let info = stage.failure_info.expect("the block records failure info");
    assert_eq!(info.failure_type, FailureType::InfrastructureError);
}

#[test]
fn a_failing_before_stage_check_blocks_before_provisioning() {
    let worktree = TempDir::new().unwrap();

    let (passed, stage) = gate(
        &["touch provisioned"],
        vec![failing_check()],
        worktree.path(),
    );

    assert!(!passed);
    assert_eq!(stage.status, StageStatus::Blocked);
    assert!(!worktree.path().join("provisioned").exists());
}

#[test]
fn a_snapshot_without_entries_leaves_the_gate_to_before_stage() {
    let worktree = TempDir::new().unwrap();

    let (passed, stage) = gate(&[], Vec::new(), worktree.path());

    assert!(passed);
    assert_eq!(stage.status, StageStatus::Queued);
    assert_eq!(stage.close_reason, None);
}

#[test]
fn a_passing_provision_runs_in_the_worktree() {
    let worktree = TempDir::new().unwrap();

    let (passed, stage) = gate(&["touch provisioned"], Vec::new(), worktree.path());

    assert!(passed);
    assert_eq!(stage.status, StageStatus::Queued);
    assert!(worktree.path().join("provisioned").exists());
}

#[test]
fn a_provision_that_leaves_an_unignored_file_blocks_the_stage() {
    let repo = repo_ignoring_node_modules();
    let command = "mkdir -p node_modules && touch node_modules/x && touch provisioned";

    let (passed, stage) = gate(&[command], Vec::new(), repo.path());

    assert!(!passed);
    assert_eq!(stage.status, StageStatus::Blocked);
    let info = stage.failure_info.expect("the block records failure info");
    assert_eq!(info.failure_type, FailureType::InfrastructureError);
    let reason = stage.close_reason.expect("the block records its reason");
    assert!(reason.contains("provisioned"), "{reason}");
    assert!(
        reason.contains("may write only files git ignores"),
        "{reason}"
    );
    assert!(!reason.contains("node_modules"), "{reason}");
}

#[test]
fn an_unignored_file_name_reaches_the_reason_without_its_escape_bytes() {
    let repo = repo_ignoring_node_modules();

    let (passed, stage) = gate(
        &["touch \"$(printf 'evil\\033[2Jname')\""],
        Vec::new(),
        repo.path(),
    );

    assert!(!passed);
    let reason = stage.close_reason.expect("the block records its reason");
    assert!(reason.contains("worktree: evil [2Jname;"), "{reason:?}");
    assert!(!reason.contains('\x1b'), "{reason:?}");
    let info = stage.failure_info.expect("the block records failure info");
    assert!(
        info.evidence.iter().all(|line| !line.contains('\x1b')),
        "{:?}",
        info.evidence
    );
}

/// Calling `run_provision` without the ticker thread writes no tick, so this fails.
#[test]
fn provisioning_stamps_the_daemon_tick() {
    let work = TempDir::new().unwrap();
    let worktree = TempDir::new().unwrap();
    assert!(tick::read(work.path()).unwrap().is_none());

    provision_worktree(work.path(), &[entry("true")], worktree.path()).unwrap();

    let stamped = tick::read(work.path())
        .unwrap()
        .expect("provisioning stamps the daemon tick");
    assert_eq!(stamped.phase, Some(Phase::Spawning));
}

#[test]
fn a_provision_that_writes_only_ignored_files_spawns() {
    let repo = repo_ignoring_node_modules();

    let (passed, stage) = gate(
        &["mkdir -p node_modules && touch node_modules/x"],
        Vec::new(),
        repo.path(),
    );

    assert!(passed);
    assert_eq!(stage.status, StageStatus::Queued);
    assert!(repo.path().join("node_modules").join("x").exists());
}

#[test]
fn a_retry_does_not_count_its_own_earlier_unignored_files() {
    let repo = repo_ignoring_node_modules();
    fs::write(repo.path().join("provisioned"), "").unwrap();

    let (passed, _) = gate(&["touch provisioned"], Vec::new(), repo.path());

    assert!(passed);
}

#[test]
fn the_gate_reads_the_snapshot_not_the_plan() {
    let temp = work_dir();
    let work = temp.path().join(".loom").join("work");
    let worktree = TempDir::new().unwrap();
    let plan = temp.path().join("PLAN.md");
    fs::write(
        &plan,
        "```yaml\nloom:\n  version: 2\n  provision:\n    - working_dir: \".\"\n      \
         command: \"touch from-plan\"\n  stages: []\n```\n",
    )
    .unwrap();
    let stage = queued_stage(&work, Vec::new());
    let mut orchestrator = orchestrator_for(&work, temp.path());
    write_provision_snapshot(&work, &[entry("touch from-snapshot")]).unwrap();
    let config_path = work.join("config.toml");
    let mut config = fs::read_to_string(&config_path).unwrap();
    config.push_str(&format!(
        "\n[plan]\nsource_path = {:?}\n",
        plan.display().to_string()
    ));
    fs::write(&config_path, config).unwrap();

    let passed = orchestrator
        .pre_spawn_gates_passed(&stage, worktree.path(), "main")
        .unwrap();

    assert!(passed);
    assert!(worktree.path().join("from-snapshot").exists());
    assert!(!worktree.path().join("from-plan").exists());
}

#[test]
fn a_provision_that_modifies_a_tracked_file_blocks_and_names_its_full_path() {
    let repo = repo_tracking("web/bun.lock");

    let (passed, stage) = gate(&["echo changed >> web/bun.lock"], Vec::new(), repo.path());

    assert!(!passed);
    assert_eq!(stage.status, StageStatus::Blocked);
    let reason = stage.close_reason.expect("the block records its reason");
    // The status entry is " M web/bun.lock": a trimmed listing clips it to "eb/bun.lock".
    assert!(reason.contains("worktree: web/bun.lock;"), "{reason}");
}

#[test]
fn a_pre_existing_modified_tracked_file_is_not_a_new_entry() {
    let repo = repo_tracking("a.txt");
    fs::write(repo.path().join("a.txt"), "edited before provisioning\n").unwrap();

    let (passed, stage) = gate(
        &["mkdir -p node_modules && touch node_modules/x"],
        Vec::new(),
        repo.path(),
    );

    assert!(passed);
    assert_eq!(stage.status, StageStatus::Queued);
}

#[test]
fn status_entries_keep_their_leading_space_and_rename_origins() {
    let listing = " M web/bun.lock\0R  new.txt\0old.txt\0?? added\0";

    assert_eq!(
        split_status_entries(listing),
        [" M web/bun.lock", "R  new.txt\0old.txt", "?? added"]
    );
}

#[test]
fn a_rename_in_the_worktree_column_keeps_its_origin_in_one_entry() {
    assert_eq!(
        split_status_entries(" R new.txt\0old.txt\0"),
        [" R new.txt\0old.txt"]
    );
}

#[test]
fn a_renamed_file_is_reported_by_its_new_path() {
    assert_eq!(
        unignored_reason(&["R  new.txt\0old.txt"]),
        unignored_reason(&["R  new.txt"])
    );
}
