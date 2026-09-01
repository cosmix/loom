//! Unit tests for orphan agent adoption (`adopt_orphaned_agents`).
//!
//! Split out of `recovery.rs`'s `mod tests` to keep that file under the
//! maintainability limit — the same trick `tests_session_registry.rs` and
//! `stage_executor_tests.rs` use.

use super::*;
use tempfile::TempDir;

use crate::models::session::{SessionBackendKind, TerminalConfig};
use crate::orchestrator::core::OrchestratorConfig;
use crate::orchestrator::terminal::native::write_test_pid_identity;
use crate::plan::ExecutionGraph;
use crate::verify::transitions::{load_stage, save_stage};

/// Set up a `.loom/work` directory with an Executing stage and a live agent —
/// a PID file naming this process, but no session record: the daemon-death
/// window this whole module exists to close.
fn orphan_test_fixture() -> (TempDir, std::path::PathBuf, Session) {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path().join(".loom").join("work");
    std::fs::create_dir_all(&work_dir).unwrap();
    // Configure the tmux lane so `Orchestrator::new` does not run terminal
    // detection, which fails on a headless runner. The adopted session is
    // still native; liveness for it degrades to the PID identity check,
    // which is the layer under test either way.
    crate::fs::work_dir::write_terminal_config(
        &work_dir,
        &TerminalConfig {
            backend: SessionBackendKind::Tmux,
        },
    )
    .unwrap();

    let mut stage = Stage::new("alpha".to_string(), None);
    stage.id = "alpha".to_string();
    stage.status = StageStatus::Executing;
    stage.session = None;
    save_stage(&stage, &work_dir).unwrap();

    let mut agent = Session::new();
    agent.assign_to_stage("alpha".to_string());
    write_test_pid_identity(&work_dir, &agent, std::process::id()).unwrap();

    (temp, work_dir, agent)
}

#[test]
fn adoption_relinks_an_executing_stage_and_is_a_noop_on_the_next_pass() {
    let (temp, work_dir, agent) = orphan_test_fixture();

    let config = OrchestratorConfig {
        work_dir: work_dir.clone(),
        repo_root: temp.path().to_path_buf(),
        enable_skill_routing: false,
        ..Default::default()
    };
    let mut orchestrator =
        Orchestrator::new(config, ExecutionGraph::build(Vec::new()).unwrap()).unwrap();

    assert_eq!(orchestrator.adopt_orphaned_agents(), 1);
    assert_eq!(
        load_stage("alpha", &work_dir).unwrap().session.as_deref(),
        Some(agent.id.as_str()),
        "the stage must name the adopted session or attach still cannot find it"
    );
    assert_eq!(
        orchestrator
            .active_sessions
            .get("alpha")
            .map(|s| s.id.as_str()),
        Some(agent.id.as_str()),
        "the monitor only watches sessions registered as active"
    );

    // Runs every tick: the record written above must make the second pass
    // find nothing rather than re-adopt and re-log forever.
    assert_eq!(orchestrator.adopt_orphaned_agents(), 0);
}
