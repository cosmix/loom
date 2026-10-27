//! Recovery against the target guard. A final stage (`Completed` and merged
//! or assumed merged) is never re-verified, reverted or moved, whatever the
//! live or accepted target holds; a completed stage that is not merged yet
//! still waits on a held target through its normal merge path.

use std::path::Path;
use std::process::Command;

use serial_test::serial;
use tempfile::TempDir;

use crate::fs::permissions::scratch_home::ScratchHome;
use crate::fs::work_dir::write_terminal_config;
use crate::git::target_guard::{Hold, HoldReason, RECORD_FILE};
use crate::git::MergeBlock;
use crate::models::session::{SessionBackendKind, TerminalConfig};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::core::recovery::Recovery;
use crate::orchestrator::core::{Orchestrator, OrchestratorConfig};
use crate::orchestrator::scheduling_report::BlockReason;
use crate::plan::{schema::StageDefinition, ExecutionGraph};
use crate::verify::transitions::{load_stage, save_stage};

const ID: &str = "s";

/// Run `git` in `dir` with ambient configuration shut out, assert it
/// succeeded, and return its trimmed stdout.
fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", dir.join(".loom-test-no-global"))
        .env("GIT_CONFIG_SYSTEM", dir.join(".loom-test-no-system"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t.com")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "git {args:?} failed: {stderr}");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Commit `name` holding `text` on the checked-out branch; returns the tip.
fn commit(dir: &Path, name: &str, text: &str) -> String {
    std::fs::write(dir.join(name), text).unwrap();
    git(dir, &["add", name]);
    git(dir, &["commit", "-q", "-m", name]);
    git(dir, &["rev-parse", "HEAD"])
}

/// A repository on `main` holding `seed.txt`, with the state directories
/// excluded from its status as in a real project.
fn repo() -> TempDir {
    let repo = TempDir::new().unwrap();
    let root = repo.path();
    git(root, &["init", "-q", "-b", "main"]);
    git(root, &["config", "user.email", "t@t.com"]);
    git(root, &["config", "user.name", "t"]);
    std::fs::write(root.join(".git/info/exclude"), ".loom/\n.worktrees/\n").unwrap();
    commit(root, "seed.txt", "seed");
    repo
}

/// `loom/s` with one commit beyond `main`, and `main` checked out; returns
/// the branch tip.
fn side_commit(root: &Path) -> String {
    git(root, &["checkout", "-q", "-b", "loom/s"]);
    let work = commit(root, "work.txt", "stage work");
    git(root, &["checkout", "-q", "main"]);
    work
}

/// An orchestrator over `root` on the tmux lane, so building it never probes
/// the host for a terminal emulator.
fn orchestrator(root: &Path) -> Orchestrator {
    let work_dir = root.join(".loom").join("work");
    let backend = SessionBackendKind::Tmux;
    write_terminal_config(&work_dir, &TerminalConfig { backend }).unwrap();
    let config = OrchestratorConfig {
        work_dir,
        repo_root: root.to_path_buf(),
        base_branch: Some("main".to_string()),
        enable_skill_routing: false,
        ..Default::default()
    };
    let definition = StageDefinition {
        id: ID.to_string(),
        name: ID.to_string(),
        working_dir: ".".into(),
        ..Default::default()
    };
    Orchestrator::new(config, ExecutionGraph::build(vec![definition]).unwrap()).unwrap()
}

/// Save a completed stage at `commit` with the given merge flags.
fn save_completed(orchestrator: &Orchestrator, commit: &str, merged: bool, merge_assumed: bool) {
    let stage = Stage {
        id: ID.to_string(),
        status: StageStatus::Completed,
        merged,
        merge_assumed,
        completed_commit: Some(commit.to_string()),
        ..Stage::default()
    };
    save_stage(&stage, &orchestrator.config.work_dir).unwrap();
}

/// Save a completed stage flagged `merged` at `commit`.
fn save_merged(orchestrator: &Orchestrator, commit: &str) {
    save_completed(orchestrator, commit, true, false);
}

/// Sync, then require the stage to be exactly as final as it was saved.
fn assert_still_final(orchestrator: &mut Orchestrator, merged: bool, why: &str) {
    orchestrator.sync_graph_with_stage_files().unwrap();
    let stage = load_stage(ID, &orchestrator.config.work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::Completed, "{why}");
    assert_eq!(stage.merged, merged, "{why}");
    assert!(
        stage.merge.block.is_none(),
        "{why}: {:?}",
        stage.merge.block
    );
    assert!(
        stage.failure_info.is_none(),
        "{why}: {:?}",
        stage.failure_info
    );
}

#[test]
#[serial]
fn an_unreadable_guard_record_keeps_a_merged_stage_merged() {
    let repo = repo();
    let root = repo.path();
    let mut orchestrator = orchestrator(root);
    save_merged(&orchestrator, &git(root, &["rev-parse", "main"]));
    let record = orchestrator.config.work_dir.join(RECORD_FILE);
    std::fs::write(record, "{ not json").unwrap();

    orchestrator.sync_graph_with_stage_files().unwrap();

    let stage = load_stage(ID, &orchestrator.config.work_dir).unwrap();
    assert!(stage.merged, "an unreadable guard record reverted merged");
}

#[test]
#[serial]
fn a_final_merged_stage_outside_the_accepted_tip_stays_merged() {
    let repo = repo();
    let root = repo.path();
    let side = side_commit(root);
    let mut orchestrator = orchestrator(root);
    assert_eq!(orchestrator.check_target_guard(), None);
    assert!(orchestrator.config.work_dir.join(RECORD_FILE).exists());
    save_merged(&orchestrator, &side);

    let why = "a merged stage whose commit is not in the accepted tip was reopened";
    assert_still_final(&mut orchestrator, true, why);
}

/// `main` moved onto `commit` the way an agent would, with hooks off.
fn agent_move(root: &Path, commit: &str) {
    let args = ["-c", "core.hooksPath=/dev/null", "update-ref"];
    git(root, &[&args[..], &["refs/heads/main", commit]].concat());
}

/// The incident's shape: `main` moved outside loom onto the tip of `loom/s`,
/// so the guard holds it as carrying that stage's work. Returns the hold.
fn hold_a_move_carrying_stage_work(root: &Path, orchestrator: &mut Orchestrator) -> Hold {
    let side = side_commit(root);
    assert_eq!(orchestrator.check_target_guard(), None);
    agent_move(root, &side);
    let hold = orchestrator
        .check_target_guard()
        .expect("the move was not held");
    assert_eq!(hold.observed, side);
    let stage_work = HoldReason::StageWork {
        branches: vec!["loom/s".to_string()],
    };
    assert!(hold.reasons.contains(&stage_work), "{hold:?}");
    hold
}

#[test]
#[serial]
fn a_held_move_carrying_assumed_work_leaves_the_stage_final() {
    let repo = repo();
    let root = repo.path();
    let mut orchestrator = orchestrator(root);
    let hold = hold_a_move_carrying_stage_work(root, &mut orchestrator);
    save_completed(&orchestrator, &hold.observed, true, true);

    let why = "a held target moved an assume-merged stage";
    assert_still_final(&mut orchestrator, true, why);
    let stage = load_stage(ID, &orchestrator.config.work_dir).unwrap();
    assert!(stage.merge_assumed, "{why}");
}

#[test]
#[serial]
fn an_assumed_stage_recorded_unmerged_is_not_merged_again() {
    let repo = repo();
    let root = repo.path();
    let mut orchestrator = orchestrator(root);
    let hold = hold_a_move_carrying_stage_work(root, &mut orchestrator);
    save_completed(&orchestrator, &hold.observed, false, true);

    let why = "an assume-merged stage went through auto-merge";
    assert_still_final(&mut orchestrator, false, why);
}

#[test]
#[serial]
fn an_unmerged_completed_stage_still_waits_on_the_held_target() {
    let repo = repo();
    let root = repo.path();
    let mut orchestrator = orchestrator(root);
    let hold = hold_a_move_carrying_stage_work(root, &mut orchestrator);
    save_completed(&orchestrator, &hold.observed, false, false);

    orchestrator.sync_graph_with_stage_files().unwrap();

    let stage = load_stage(ID, &orchestrator.config.work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::MergeBlocked);
    assert!(!stage.merged);
    assert!(
        matches!(stage.merge.block, Some(MergeBlock::TargetHeld { .. })),
        "{:?}",
        stage.merge.block
    );
}

#[test]
#[serial]
fn a_rewrite_dropping_a_merged_commit_keeps_it_merged() {
    let repo = repo();
    let root = repo.path();
    let seed = git(root, &["rev-parse", "main"]);
    let merged = commit(root, "merged.txt", "merged work");
    let mut orchestrator = orchestrator(root);
    assert_eq!(orchestrator.check_target_guard(), None);
    git(root, &["reset", "-q", "--hard", &seed]);
    let rewritten = commit(root, "rewrite.txt", "rewritten history");
    let hold = orchestrator
        .check_target_guard()
        .expect("the rewrite was not held");
    assert_eq!(hold.observed, rewritten);
    assert!(
        hold.reasons.contains(&HoldReason::NotFastForward),
        "{hold:?}"
    );
    save_merged(&orchestrator, &merged);

    orchestrator.sync_graph_with_stage_files().unwrap();

    let stage = load_stage(ID, &orchestrator.config.work_dir).unwrap();
    assert!(
        stage.merged,
        "a rewrite of the live target reverted a merge the accepted tip holds"
    );
}

#[test]
#[serial]
fn an_unreadable_guard_record_blocks_a_new_stage_spawn() {
    let repo = repo();
    let root = repo.path();
    let mut orchestrator = orchestrator(root);
    let record = orchestrator.config.work_dir.join(RECORD_FILE);
    std::fs::write(record, "{ not json").unwrap();
    let stage = Stage {
        id: ID.to_string(),
        status: StageStatus::Queued,
        ..Stage::default()
    };
    let _home = ScratchHome::set();

    let resolved = orchestrator.resolve_worktree(ID, &stage).unwrap();

    assert!(resolved.is_none(), "a worktree was created");
    assert_eq!(
        orchestrator.spawn_blocks.get(ID),
        Some(&BlockReason::TargetHeld),
        "the spawn was not held for the target"
    );
    let branch = Command::new("git")
        .args(["rev-parse", "--verify", "--quiet", "refs/heads/loom/s"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(!branch.status.success(), "loom/s was created");
}

#[test]
#[serial]
fn a_repeating_probe_error_is_kept_once_until_a_probe_answers() {
    let repo = repo();
    let root = repo.path();
    let mut orchestrator = orchestrator(root);
    let main = git(root, &["rev-parse", "main"]);
    let record = orchestrator.config.work_dir.join(RECORD_FILE);
    std::fs::write(&record, "{ not json").unwrap();

    assert!(orchestrator.probe_accepted(ID, &main, "main").is_err());
    let logged = orchestrator.merge_probe_error.clone();
    assert!(logged.is_some(), "the probe error was not logged");
    assert!(orchestrator.probe_accepted("t", &main, "main").is_err());
    assert_eq!(orchestrator.merge_probe_error, logged);

    std::fs::remove_file(&record).unwrap();
    assert!(orchestrator.probe_accepted(ID, &main, "main").unwrap());
    assert_eq!(orchestrator.merge_probe_error, None);
}
