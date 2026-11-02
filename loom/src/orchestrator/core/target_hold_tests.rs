//! The daemon's use of the target guard, with real git: the in-memory hold,
//! what it stops (merge retries, resolvers, knowledge stages), where new
//! stage worktrees start while the target is held, and recovery's ancestry
//! check against the accepted tip.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use serial_test::serial;
use tempfile::TempDir;

use crate::fs::permissions::scratch_home::ScratchHome;
use crate::fs::work_dir::write_terminal_config;
use crate::git::hooks::is_reference_transaction_hook_installed;
use crate::git::merge::lock::MergeLock;
use crate::git::target_guard::{accept, accepted_tip};
use crate::git::{get_or_create_worktree, MergeBlock};
use crate::models::session::{SessionBackendKind, TerminalConfig};
use crate::models::stage::{Stage, StageStatus, StageType};
use crate::orchestrator::core::recovery::Recovery;
use crate::orchestrator::core::stage_executor::StageExecutor;
use crate::orchestrator::core::{merge_resolver_attempts, Orchestrator, OrchestratorConfig};
use crate::orchestrator::scheduling_report::{alerts, BlockReason, Severity};
use crate::plan::{schema::StageDefinition, ExecutionGraph};
use crate::verify::transitions::{load_stage, save_stage};

pub(super) const ID: &str = "s";

/// Run `git` in `dir` with ambient configuration shut out, assert it
/// succeeded, and return its trimmed stdout.
pub(super) fn git(dir: &Path, args: &[&str]) -> String {
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

pub(super) fn tip(root: &Path, name: &str) -> String {
    git(root, &["rev-parse", name])
}

/// Commit `name` holding `text` on the checked-out branch; returns the tip.
pub(super) fn commit(dir: &Path, name: &str, text: &str) -> String {
    let file = dir.join(name);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
    git(dir, &["add", name]);
    git(dir, &["commit", "-q", "-m", name]);
    tip(dir, "HEAD")
}

/// A repository on `main` holding `seed.txt`, with the state directories
/// excluded from its status as in a real project.
pub(super) fn repo() -> TempDir {
    let repo = TempDir::new().unwrap();
    let root = repo.path();
    git(root, &["init", "-q", "-b", "main"]);
    git(root, &["config", "user.email", "t@t.com"]);
    git(root, &["config", "user.name", "t"]);
    std::fs::write(root.join(".git/info/exclude"), ".loom/\n.worktrees/\n").unwrap();
    commit(root, "seed.txt", "seed");
    let work_dir = root.join(".loom").join("work");
    std::fs::create_dir_all(&work_dir).unwrap();
    crate::git::worktree::capture_main_identity(root, &work_dir).unwrap();
    repo
}

/// `loom/s` with one commit beyond `main`, and `main` checked out; returns
/// the branch tip.
pub(super) fn stage_branch(root: &Path) -> String {
    git(root, &["checkout", "-q", "-b", "loom/s"]);
    let work = commit(root, "work.txt", "stage work");
    git(root, &["checkout", "-q", "main"]);
    work
}

pub(super) fn graph(ids: &[&str]) -> ExecutionGraph {
    let definitions = ids.iter().map(|id| StageDefinition {
        id: id.to_string(),
        name: id.to_string(),
        working_dir: ".".into(),
        ..Default::default()
    });
    ExecutionGraph::build(definitions.collect()).unwrap()
}

/// An orchestrator over `root` on the tmux lane, so building it never probes
/// the host for a terminal emulator.
pub(super) fn orchestrator(root: &Path, graph: ExecutionGraph) -> Orchestrator {
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
    Orchestrator::new(config, graph).unwrap()
}

/// An orchestrator whose guard recorded `main`, then saw it moved by a
/// commit of `.claude/settings.json`, a control path held whatever the
/// attestation mode. Returns it with the accepted and the moved tip.
pub(super) fn held(root: &Path, graph: ExecutionGraph) -> (Orchestrator, String, String) {
    let mut orchestrator = orchestrator(root, graph);
    assert_eq!(orchestrator.check_target_guard(), None);
    let accepted = tip(root, "main");
    let moved = commit(root, ".claude/settings.json", "{}\n");
    assert!(
        orchestrator.check_target_guard().is_some(),
        "the move was not held"
    );
    (orchestrator, accepted, moved)
}

#[test]
fn a_repeated_hold_is_returned_unchanged_and_logs_no_error() {
    let repo = repo();
    let (mut orchestrator, accepted, moved) = held(repo.path(), graph(&[]));
    let first = orchestrator.target_hold.clone().expect("the hold is kept");

    assert_eq!(orchestrator.check_target_guard(), Some(first.clone()));
    assert_eq!((first.accepted, first.observed), (accepted, moved));
    assert!(orchestrator.target_guard_error.is_none());
    assert!(orchestrator.target_held());
}

#[test]
fn a_contended_check_of_a_restored_target_clears_the_hold() {
    let repo = repo();
    let root = repo.path();
    let (mut orchestrator, accepted, _) = held(root, graph(&[]));
    git(root, &["reset", "-q", "--hard", &accepted]);
    let work_dir = orchestrator.config.work_dir.clone();

    let lock = MergeLock::acquire(&work_dir, Duration::from_secs(5)).unwrap();
    let contended = orchestrator.check_target_guard();
    lock.release().unwrap();

    assert_eq!(contended, None, "a contended check kept a restored hold");
    assert_eq!(orchestrator.check_target_guard(), None);
}

#[test]
fn spawn_hold_reason_stops_held_and_knowledge_stages_only() {
    let repo = repo();
    let (orchestrator, _, _) = held(repo.path(), graph(&[]));
    let knowledge = Stage {
        stage_type: StageType::Knowledge,
        ..Stage::default()
    };
    let on_hold = Stage {
        held: true,
        ..Stage::default()
    };

    let reason = |stage: &Stage| orchestrator.spawn_hold_reason(stage);
    assert_eq!(reason(&knowledge), Some(BlockReason::TargetHeld));
    assert_eq!(reason(&Stage::default()), None);
    assert_eq!(reason(&on_hold), Some(BlockReason::Held));
}

#[test]
#[serial]
fn a_new_worktree_starts_at_the_given_start_point() {
    let repo = repo();
    let root = repo.path();
    let (_orchestrator, accepted, moved) = held(root, graph(&[]));
    let _home = ScratchHome::set();

    get_or_create_worktree(ID, root, Some("main"), Some(&accepted)).unwrap();

    assert_eq!(tip(root, "loom/s"), accepted);
    assert_eq!(tip(root, "main"), moved);
}

#[test]
fn alerts_name_a_recorded_hold_while_the_daemon_is_down() {
    let repo = repo();
    let (orchestrator, _, moved) = held(repo.path(), graph(&[]));

    let shown = alerts(&orchestrator.config.work_dir, false);

    assert_eq!(shown.len(), 1, "{shown:?}");
    assert_eq!(shown[0].severity, Severity::Warning);
    assert!(shown[0].text.contains(&moved[..7]), "{shown:?}");
}

#[test]
fn a_target_held_stage_is_retried_only_after_accept() {
    let repo = repo();
    let root = repo.path();
    let work = stage_branch(root);
    let completed = Stage {
        id: ID.to_string(),
        status: StageStatus::Completed,
        completed_commit: Some(work.clone()),
        ..Stage::default()
    };
    let (mut orchestrator, accepted, moved) = held(root, graph(&[]));
    let work_dir = orchestrator.config.work_dir.clone();
    save_stage(&completed, &work_dir).unwrap();
    assert!(!orchestrator.try_auto_merge(ID));
    let block = MergeBlock::TargetHeld {
        target: "main".to_string(),
        accepted,
        observed: moved.clone(),
    };
    assert_eq!(load_stage(ID, &work_dir).unwrap().merge.block, Some(block));

    assert_eq!(orchestrator.spawn_merge_resolution_sessions().unwrap(), 0);
    assert_eq!(tip(root, "main"), moved, "a held retry moved the target");

    accept(root, &work_dir, "main", &moved).unwrap();
    assert_eq!(orchestrator.check_target_guard(), None);
    orchestrator.spawn_merge_resolution_sessions().unwrap();

    assert!(
        load_stage(ID, &work_dir).unwrap().merged,
        "the retry did not land"
    );
    git(root, &["merge-base", "--is-ancestor", &work, "main"]);
}

#[test]
#[serial]
fn a_held_respawn_keeps_an_orphaned_stage_branch() {
    let repo = repo();
    let root = repo.path();
    let accepted = tip(root, "main");
    let work = stage_branch(root);
    let mut orchestrator = orchestrator(root, graph(&[]));
    assert_eq!(orchestrator.check_target_guard(), None);
    let args = [
        "-c",
        "core.hooksPath=/dev/null",
        "update-ref",
        "refs/heads/main",
    ];
    git(root, &[&args[..], &[work.as_str()]].concat());
    assert!(
        orchestrator.check_target_guard().is_some(),
        "the move was not held"
    );
    let _home = ScratchHome::set();

    get_or_create_worktree(ID, root, Some("main"), Some(&accepted)).unwrap();

    assert_eq!(
        tip(root, "loom/s"),
        work,
        "the stage branch lost its commit"
    );
    assert!(root.join(".worktrees").join(ID).is_dir());
}

#[test]
#[serial]
fn resolve_worktree_starts_at_the_accepted_tip_while_held() {
    let repo = repo();
    let root = repo.path();
    let (mut orchestrator, accepted, moved) = held(root, graph(&[ID]));
    let stage = Stage {
        id: ID.to_string(),
        status: StageStatus::Queued,
        ..Stage::default()
    };
    let _home = ScratchHome::set();

    let resolved = orchestrator.resolve_worktree(ID, &stage).unwrap();

    assert!(resolved.is_some(), "no worktree was created");
    assert_eq!(tip(root, "loom/s"), accepted);
    assert_ne!(tip(root, "loom/s"), moved);
}

#[test]
fn a_conflicted_stage_gets_no_resolver_while_held() {
    let repo = repo();
    let (mut orchestrator, _, _) = held(repo.path(), graph(&[]));
    let work_dir = orchestrator.config.work_dir.clone();
    let conflicted = Stage {
        id: ID.to_string(),
        status: StageStatus::MergeConflict,
        ..Stage::default()
    };
    save_stage(&conflicted, &work_dir).unwrap();

    assert_eq!(orchestrator.spawn_merge_resolution_sessions().unwrap(), 0);

    assert_eq!(merge_resolver_attempts(&work_dir, ID), 0);
    let status = load_stage(ID, &work_dir).unwrap().status;
    assert_eq!(
        status,
        StageStatus::MergeConflict,
        "a held stage was routed"
    );
}

#[test]
fn a_knowledge_stage_is_not_started_while_held() {
    let repo = repo();
    let (mut orchestrator, _, _) = held(repo.path(), graph(&[]));
    let work_dir = orchestrator.config.work_dir.clone();
    let knowledge = Stage {
        id: "k".to_string(),
        status: StageStatus::Queued,
        stage_type: StageType::Knowledge,
        ..Stage::default()
    };
    save_stage(&knowledge, &work_dir).unwrap();

    orchestrator.start_stage("k").unwrap();

    assert_eq!(
        load_stage("k", &work_dir).unwrap().status,
        StageStatus::Queued
    );
    let reason = orchestrator.spawn_blocks.get("k");
    assert_eq!(reason, Some(&BlockReason::TargetHeld));
}

#[test]
fn start_target_guard_installs_the_hook_and_records_the_tip() {
    let repo = repo();
    let root = repo.path();
    let mut orchestrator = orchestrator(root, graph(&[]));

    orchestrator.start_target_guard();

    assert!(is_reference_transaction_hook_installed(root));
    let recorded = accepted_tip(&orchestrator.config.work_dir, "main").unwrap();
    assert_eq!(recorded, Some(tip(root, "main")));
    assert!(!orchestrator.target_held());
}

#[test]
fn a_graft_does_not_mark_a_stage_merged() {
    let repo = repo();
    let root = repo.path();
    let work = stage_branch(root);
    let main = tip(root, "main");
    let mut orchestrator = orchestrator(root, graph(&[ID]));
    let stage = Stage {
        id: ID.to_string(),
        status: StageStatus::Completed,
        completed_commit: Some(work.clone()),
        auto_merge: Some(false),
        ..Stage::default()
    };
    save_stage(&stage, &orchestrator.config.work_dir).unwrap();
    std::fs::write(root.join(".git/info/grafts"), format!("{main} {work}\n")).unwrap();

    orchestrator.sync_graph_with_stage_files().unwrap();
    orchestrator.sweep_merged_leftovers();

    let stage = load_stage(ID, &orchestrator.config.work_dir).unwrap();
    assert!(!stage.merged, "a graft made the stage look merged");
    assert_eq!(tip(root, "loom/s"), work);
    assert_eq!(tip(root, "main"), main);
}
