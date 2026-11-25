//! Contracts for workers that died where the daemon could not see them: a
//! `WaitingForInput` stage whose session crashed, and a `Spawning` record
//! too young to judge at restart whose launch never produced a process.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use chrono::Utc;
use serial_test::serial;
use tempfile::TempDir;

use crate::fs::session_files::save_session;
use crate::models::session::{Session, SessionStatus};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::core::{Orchestrator, OrchestratorConfig};
use crate::orchestrator::liveness::LivenessService;
use crate::orchestrator::terminal::launch::{tracking_and_pid_key, wrapper_script_path};
use crate::plan::{schema::StageDefinition, ExecutionGraph};
use crate::verify::transitions::{load_stage, save_stage};

fn work_dir(temp: &TempDir) -> PathBuf {
    let work_dir = temp.path().join(".loom").join("work");
    std::fs::create_dir_all(&work_dir).unwrap();
    work_dir
}

fn orchestrator(temp: &TempDir, graph: ExecutionGraph) -> Orchestrator {
    let config = OrchestratorConfig {
        work_dir: work_dir(temp),
        repo_root: temp.path().to_path_buf(),
        enable_skill_routing: false,
        ..Default::default()
    };
    Orchestrator::new(config, graph).unwrap()
}

fn stage_under(work_dir: &Path, id: &str, status: StageStatus, session: &str) {
    let stage = Stage {
        id: id.to_string(),
        name: id.to_string(),
        status,
        session: Some(session.to_string()),
        ..Stage::default()
    };
    save_stage(&stage, work_dir).unwrap();
}

fn handle(id: &str, stage_id: &str, age: chrono::Duration) -> Session {
    let mut session = Session::new();
    session.assign_to_stage(stage_id.to_string());
    session.id = id.to_string();
    session.status = SessionStatus::Running;
    session.created_at = Utc::now() - age;
    session
}

/// The crashed session of a `WaitingForInput` stage gives up its slot and
/// blocks the stage; a crash event from an older session of another waiting
/// stage changes nothing.
#[test]
fn a_dead_waiter_is_blocked_and_its_slot_freed() {
    let temp = tempfile::tempdir().unwrap();
    let mut orchestrator = orchestrator(&temp, ExecutionGraph::build(Vec::new()).unwrap());
    let work = orchestrator.config.work_dir.clone();
    stage_under(&work, "weather", StageStatus::WaitingForInput, "s-dead");
    stage_under(&work, "radar", StageStatus::WaitingForInput, "s-live");
    let ten_minutes = chrono::Duration::minutes(10);
    let dead = handle("s-dead", "weather", ten_minutes);
    let live = handle("s-live", "radar", ten_minutes);
    orchestrator.active_sessions.insert("weather".into(), dead);
    orchestrator.active_sessions.insert("radar".into(), live);

    orchestrator
        .handle_session_crashed("s-dead", Some("weather".into()), None)
        .unwrap();
    orchestrator
        .handle_session_crashed("s-old", Some("radar".into()), None)
        .unwrap();

    let weather = load_stage("weather", &work).unwrap();
    assert_eq!(weather.status, StageStatus::Blocked);
    assert!(
        !orchestrator.active_sessions.contains_key("weather"),
        "the dead waiter keeps its handle and its scheduler slot"
    );
    let radar = load_stage("radar", &work).unwrap();
    assert_eq!(radar.status, StageStatus::WaitingForInput);
    assert_eq!(radar.session.as_deref(), Some("s-live"));
    assert_eq!(
        orchestrator
            .active_sessions
            .get("radar")
            .map(|s| s.id.as_str()),
        Some("s-live"),
        "a stale crash event dropped a live waiter's handle"
    );
}

fn executing_graph(id: &str) -> ExecutionGraph {
    let definition = StageDefinition {
        id: id.to_string(),
        name: id.to_string(),
        working_dir: ".".into(),
        ..Default::default()
    };
    let mut graph = ExecutionGraph::build(vec![definition]).unwrap();
    graph.mark_queued(id).unwrap();
    graph.mark_executing(id).unwrap();
    graph
}

fn backdate_wrapper(path: &Path, by: Duration) {
    let file = std::fs::OpenOptions::new().write(true).open(path).unwrap();
    file.set_modified(SystemTime::now() - by).unwrap();
}

/// A `Spawning` record that wrote its wrapper and has no process is too
/// young to judge at restart; once record and wrapper pass the grace, a
/// later pass retires it.
#[test]
#[serial]
fn a_young_spawning_record_is_retired_on_a_later_pass() {
    let temp = tempfile::tempdir().unwrap();
    let mut orchestrator = orchestrator(&temp, executing_graph("alpha"));
    orchestrator.liveness = LivenessService::fixed_for_tests(false);
    let work = orchestrator.config.work_dir.clone();
    let mut session = Session::new();
    session.assign_to_stage("alpha".to_string());
    save_session(&session, &work).unwrap();
    stage_under(&work, "alpha", StageStatus::Executing, &session.id);
    let (_, pid_key) = tracking_and_pid_key(&session).unwrap();
    let wrapper = wrapper_script_path(&work, &pid_key);
    std::fs::create_dir_all(wrapper.parent().unwrap()).unwrap();
    std::fs::write(&wrapper, "").unwrap();

    orchestrator.recover_current_sessions().unwrap();

    let young = load_stage("alpha", &work).unwrap();
    assert_eq!(young.status, StageStatus::Executing);
    assert_eq!(young.session.as_deref(), Some(session.id.as_str()));
    assert_eq!(
        orchestrator.deferred_orphans.get("alpha"),
        Some(&session.id)
    );

    session.created_at -= chrono::Duration::seconds(120);
    save_session(&session, &work).unwrap();
    backdate_wrapper(&wrapper, Duration::from_secs(120));
    orchestrator.recover_deferred_orphans();

    let retired = load_stage("alpha", &work).unwrap();
    assert_ne!(retired.status, StageStatus::Executing);
    assert_eq!(retired.session, None);
    assert!(orchestrator.deferred_orphans.is_empty());
}
