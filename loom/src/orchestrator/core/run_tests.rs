//! What one `run_tick` does that no step's own tests can show: that the loop
//! reaches the step at all, in the mode that polls no monitor.

use serial_test::serial;
use tempfile::TempDir;

use super::*;
use crate::fs::session_files::save_session;
use crate::fs::work_dir::write_terminal_config;
use crate::models::session::{Session, SessionBackendKind, SessionStatus, TerminalConfig};
use crate::models::stage::{Stage, StageStatus, StageType};
use crate::orchestrator::core::OrchestratorConfig;
use crate::plan::schema::StageDefinition;
use crate::plan::ExecutionGraph;
use crate::verify::contracts::refusal::{daemon_refusal, record_daemon_refusal, RefusalSource};
use crate::verify::contracts::test_support::contract;
use crate::verify::transitions::{create_stage, load_stage};

const STAGE: &str = "test-stage";

/// A manual-mode orchestrator over a v2 standard stage executing its contract
/// phase under the running writer `writer`.
struct Manual {
    _temp: TempDir,
    work: std::path::PathBuf,
    writer: Session,
    orchestrator: Orchestrator,
}

fn manual() -> Manual {
    let temp = TempDir::new().unwrap();
    let work = temp.path().join(".loom").join("work");
    std::fs::create_dir_all(&work).unwrap();
    // A tmux lane, so `Orchestrator::new` runs no terminal detection, which
    // fails on a headless runner.
    let terminal = TerminalConfig {
        backend: SessionBackendKind::Tmux,
    };
    write_terminal_config(&work, &terminal).unwrap();
    let mut writer = Session::new_contract(STAGE);
    writer.status = SessionStatus::Running;
    writer.backend = SessionBackendKind::Tmux;
    save_session(&writer, &work).unwrap();
    let stage = Stage {
        id: STAGE.to_string(),
        name: "Test Stage".to_string(),
        status: StageStatus::Executing,
        session: Some(writer.id.clone()),
        working_dir: Some(".".to_string()),
        plan_version: 2,
        stage_type: StageType::Standard,
        contracts: vec![contract()],
        ..Stage::default()
    };
    create_stage(&stage, &work).unwrap();
    let definition = StageDefinition {
        id: STAGE.to_string(),
        name: "Test Stage".to_string(),
        working_dir: ".".to_string(),
        ..Default::default()
    };
    let config = OrchestratorConfig {
        work_dir: work.clone(),
        repo_root: temp.path().to_path_buf(),
        enable_skill_routing: false,
        manual_mode: true,
        ..Default::default()
    };
    let graph = ExecutionGraph::build(vec![definition]).unwrap();
    let mut orchestrator = Orchestrator::new(config, graph).unwrap();
    orchestrator.graph.mark_executing(STAGE).unwrap();
    Manual {
        _temp: temp,
        work,
        writer,
        orchestrator,
    }
}

/// Manual mode polls no monitor, so a refused freeze is filed by `run_tick`
/// itself: a call that moved behind the monitor poll would leave the refused
/// stage executing for good.
#[test]
#[serial]
fn a_manual_tick_files_a_refused_contract_freeze() {
    let mut fx = manual();
    let problems = vec!["src/lib.rs is not a contract file".to_string()];
    record_daemon_refusal(
        &fx.work,
        STAGE,
        &fx.writer.id,
        &problems,
        RefusalSource::Daemon,
    )
    .unwrap();

    fx.orchestrator.run_tick(&mut false).unwrap();

    let stage = load_stage(STAGE, &fx.work).unwrap();
    assert_eq!(stage.status, StageStatus::NeedsAdjudication);
    assert_eq!(stage.tally.freeze_refusal_disputes, 1);
    let requests = std::fs::read_dir(fx.work.join("disputes").join(STAGE))
        .unwrap()
        .filter(|entry| entry.as_ref().unwrap().path().join("request.md").is_file())
        .count();
    assert_eq!(requests, 1);
    assert_eq!(daemon_refusal(&fx.work, STAGE).unwrap(), None);
}
