//! Spawn publication: a failed spawn keeps the retry's work, a hold stops a
//! launch that was already provisioning, and a spawn never leaves a session
//! running untracked beside a tracked one.

use std::path::Path;

use chrono::{Duration, Utc};
use tempfile::TempDir;

use crate::fs::session_files::save_session;
use crate::git::get_or_create_worktree;
use crate::models::session::{Session, SessionStatus};
use crate::models::stage::{Stage, StageStatus, StageType};
use crate::orchestrator::core::{Orchestrator, OrchestratorConfig};
use crate::orchestrator::scheduling_report::BlockReason;
use crate::orchestrator::terminal::launch::write_test_pid_identity;
use crate::plan::{schema::StageDefinition, ExecutionGraph};
use crate::test_git::{commit_file, git, TestRepo};
use crate::verify::transitions::{load_stage, save_stage};

/// A repository on `main` holding `seed.txt`, with the state and worktree
/// directories excluded from its status as in a real project.
fn repo() -> TempDir {
    let repo = TestRepo::builder()
        .identity("t", "t@t.com")
        .no_seed()
        .create()
        .into_temp_dir();
    let root = repo.path();
    std::fs::write(root.join(".git/info/exclude"), ".loom/\n.worktrees/\n").unwrap();
    commit_file(root, "seed.txt", "seed", "seed.txt");
    std::fs::create_dir_all(root.join(".loom").join("work")).unwrap();
    repo
}

fn graph(ids: &[&str]) -> ExecutionGraph {
    let definitions = ids.iter().map(|id| StageDefinition {
        id: id.to_string(),
        name: id.to_string(),
        working_dir: ".".into(),
        ..Default::default()
    });
    ExecutionGraph::build(definitions.collect()).unwrap()
}

/// An orchestrator over `root`.
fn orchestrator(root: &Path, ids: &[&str]) -> Orchestrator {
    let work_dir = root.join(".loom").join("work");
    std::fs::create_dir_all(&work_dir).unwrap();
    let config = OrchestratorConfig {
        work_dir,
        repo_root: root.to_path_buf(),
        base_branch: Some("main".to_string()),
        enable_skill_routing: false,
        ..Default::default()
    };
    Orchestrator::new(config, graph(ids)).unwrap()
}

fn stage(id: &str, status: StageStatus, session: Option<&str>) -> Stage {
    Stage {
        id: id.to_string(),
        name: id.to_string(),
        status,
        session: session.map(str::to_string),
        ..Stage::default()
    }
}

fn session_for(stage_id: &str) -> Session {
    let mut session = Session::new();
    session.assign_to_stage(stage_id.to_string());
    session.status = SessionStatus::Running;
    session
}

/// Save a write-ahead record for `stage_id` and return its file.
fn write_ahead_record(work_dir: &Path, stage_id: &str) -> (Session, std::path::PathBuf) {
    let session = session_for(stage_id);
    save_session(&session, work_dir).unwrap();
    let record = work_dir.join("sessions").join(format!("{}.md", session.id));
    assert!(record.exists(), "the write-ahead record was not saved");
    (session, record)
}

#[test]
fn a_failed_spawn_keeps_the_branch_commits_index_and_untracked_files() {
    let repo = repo();
    let root = repo.path();
    let worktree = get_or_create_worktree("s", root, Some("main"), None).unwrap();
    let tree = worktree.path.clone();
    commit_file(&tree, "work.txt", "committed work", "work.txt");
    std::fs::write(tree.join("seed.txt"), "staged edit").unwrap();
    git(&tree, &["add", "seed.txt"]);
    std::fs::write(tree.join("notes.txt"), "untracked notes").unwrap();
    let tip = git(root, &["rev-parse", "refs/heads/loom/s"]);
    let staged_before = git(&tree, &["diff", "--cached"]);
    let mut orchestrator = orchestrator(root, &["s"]);
    let work_dir = orchestrator.config.work_dir.clone();
    save_stage(&stage("s", StageStatus::Executing, Some("sess")), &work_dir).unwrap();

    orchestrator.block_failed_spawn("s", "sess", &anyhow::anyhow!("boom"));

    assert!(tree.is_dir(), "the failed spawn removed the worktree");
    assert_eq!(git(root, &["rev-parse", "refs/heads/loom/s"]), tip);
    assert_eq!(git(&tree, &["diff", "--cached"]), staged_before);
    assert_eq!(
        std::fs::read(tree.join("notes.txt")).ok().as_deref(),
        Some(b"untracked notes".as_slice())
    );
    assert_eq!(
        load_stage("s", &work_dir).unwrap().status,
        StageStatus::Blocked
    );
}

#[test]
fn a_hold_taken_after_the_queue_check_prevents_the_launch() {
    let temp = tempfile::tempdir().unwrap();
    let mut orchestrator = orchestrator(temp.path(), &["alpha"]);
    let work_dir = orchestrator.config.work_dir.clone();
    let mut queued = stage("alpha", StageStatus::Queued, None);
    queued.held = true;
    save_stage(&queued, &work_dir).unwrap();
    let (session, record) = write_ahead_record(&work_dir, "alpha");

    let marked = orchestrator
        .mark_agent_executing("alpha", &session.id)
        .unwrap();

    assert!(marked.is_none(), "a held stage was marked executing");
    let after = load_stage("alpha", &work_dir).unwrap();
    assert_eq!(after.status, StageStatus::Queued);
    assert!(after.held, "the hold was dropped");
    assert_eq!(after.session, None);
    assert!(!record.exists(), "the write-ahead record was left behind");
    assert_eq!(
        orchestrator.spawn_blocks.get("alpha"),
        Some(&BlockReason::Held)
    );
}

#[test]
fn a_handover_is_not_refused_by_a_hold() {
    let temp = tempfile::tempdir().unwrap();
    let mut orchestrator = orchestrator(temp.path(), &["alpha"]);
    let work_dir = orchestrator.config.work_dir.clone();
    let mut executing = stage("alpha", StageStatus::Executing, Some("contract-session"));
    executing.held = true;
    save_stage(&executing, &work_dir).unwrap();
    let (session, _) = write_ahead_record(&work_dir, "alpha");

    let marked = orchestrator
        .mark_agent_executing("alpha", &session.id)
        .unwrap();

    let marked = marked.expect("a handover was refused by a hold");
    assert_eq!(marked.session.as_deref(), Some(session.id.as_str()));
}

#[test]
fn a_held_knowledge_stage_is_not_launched() {
    let repo = repo();
    let root = repo.path();
    let mut orchestrator = orchestrator(root, &["know"]);
    let work_dir = orchestrator.config.work_dir.clone();
    crate::git::worktree::capture_main_identity(root, &work_dir).unwrap();
    let mut held = stage("know", StageStatus::Queued, None);
    held.stage_type = StageType::Knowledge;
    held.held = true;
    save_stage(&held, &work_dir).unwrap();

    let launched = orchestrator.write_ahead_knowledge_session("know").unwrap();

    assert!(launched.is_none(), "a held knowledge stage was launched");
    let after = load_stage("know", &work_dir).unwrap();
    assert_eq!(after.status, StageStatus::Queued);
    assert_eq!(after.session, None);
    let records = std::fs::read_dir(work_dir.join("sessions"))
        .map(|dir| dir.flatten().count())
        .unwrap_or(0);
    assert_eq!(records, 0, "a write-ahead record was left behind");
    assert_eq!(
        orchestrator.spawn_blocks.get("know"),
        Some(&BlockReason::Held)
    );
}

#[test]
fn a_live_tracked_incumbent_defers_the_spawn() {
    let temp = tempfile::tempdir().unwrap();
    let mut orchestrator = orchestrator(temp.path(), &["alpha"]);
    let work_dir = orchestrator.config.work_dir.clone();
    save_stage(&stage("alpha", StageStatus::Queued, None), &work_dir).unwrap();
    let incumbent = session_for("alpha");
    write_test_pid_identity(&work_dir, &incumbent, std::process::id()).unwrap();
    orchestrator
        .active_sessions
        .insert("alpha".to_string(), incumbent);

    assert!(!orchestrator.tracked_incumbent_cleared("alpha"));

    assert!(orchestrator.active_sessions.contains_key("alpha"));
    assert!(matches!(
        orchestrator.spawn_blocks.get("alpha"),
        Some(BlockReason::OwnershipUncertain { .. })
    ));
}

#[test]
fn a_dead_aged_incumbent_is_dropped_before_the_spawn() {
    let temp = tempfile::tempdir().unwrap();
    let mut orchestrator = orchestrator(temp.path(), &["alpha"]);
    let work_dir = orchestrator.config.work_dir.clone();
    save_stage(&stage("alpha", StageStatus::Queued, None), &work_dir).unwrap();
    let mut incumbent = session_for("alpha");
    incumbent.created_at = Utc::now() - Duration::minutes(10);
    orchestrator
        .active_sessions
        .insert("alpha".to_string(), incumbent);

    assert!(orchestrator.tracked_incumbent_cleared("alpha"));

    assert!(!orchestrator.active_sessions.contains_key("alpha"));
    assert!(!orchestrator.spawn_blocks.contains_key("alpha"));
}

#[test]
fn a_refused_insert_blocks_the_stage() {
    let temp = tempfile::tempdir().unwrap();
    let mut orchestrator = orchestrator(temp.path(), &["alpha"]);
    let work_dir = orchestrator.config.work_dir.clone();
    save_stage(
        &stage("alpha", StageStatus::Executing, Some("new")),
        &work_dir,
    )
    .unwrap();
    let incumbent = session_for("alpha");
    write_test_pid_identity(&work_dir, &incumbent, std::process::id()).unwrap();
    let incumbent_id = incumbent.id.clone();
    orchestrator
        .active_sessions
        .insert("alpha".to_string(), incumbent);
    let replacement = session_for("alpha");
    let replacement_id = replacement.id.clone();

    assert!(!orchestrator.track_spawned_session("alpha", replacement));

    let after = load_stage("alpha", &work_dir).unwrap();
    assert_eq!(after.status, StageStatus::Blocked);
    let evidence = after.failure_info.expect("no failure evidence").evidence;
    let evidence = evidence.join("\n");
    assert!(evidence.contains(&incumbent_id), "{evidence}");
    assert!(evidence.contains(&replacement_id), "{evidence}");
    assert_eq!(orchestrator.active_sessions["alpha"].id, incumbent_id);
}
