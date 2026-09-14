//! Unit tests for [`super::live_agents_for`]: a stage's live agents must
//! exclude an adjudication session, since the judge is not an agent working
//! the stage.

use super::*;
use crate::fs::session_files::{load_session_exact, save_session};
use crate::fs::work_dir::write_terminal_config;
use crate::models::session::{
    Session, SessionBackendKind, SessionExitReason, SessionStatus, TerminalConfig,
};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::terminal::native::write_test_pid_identity;
use crate::verify::transitions::{load_stage, save_stage};
use anyhow::{bail, Result};
use std::path::Path;
use tempfile::TempDir;

/// A work directory whose configured terminal lane is tmux, so liveness
/// checks never run real terminal detection (which fails on a headless test
/// runner).
fn work_dir() -> TempDir {
    let temp = TempDir::new().unwrap();
    write_terminal_config(
        temp.path(),
        &TerminalConfig {
            backend: SessionBackendKind::Tmux,
        },
    )
    .unwrap();
    temp
}

fn session_for(stage_id: &str, status: SessionStatus) -> Session {
    let mut session = Session::new();
    session.assign_to_stage(stage_id.to_string());
    session.status = status;
    session
}

/// The PID file the wrapper script writes at spawn, naming this test process
/// so the identity probe answers `VerifiedAlive`.
fn spawn_a_live_agent(work: &std::path::Path, session: &Session) {
    write_test_pid_identity(work, session, std::process::id()).unwrap();
}

fn executing_stage(work: &Path, session: Option<&Session>) {
    let mut stage = Stage::new("alpha".to_string(), None);
    stage.id = "alpha".to_string();
    stage.status = StageStatus::Executing;
    stage.session = session.map(|item| item.id.clone());
    save_stage(&stage, work).unwrap();
}

#[derive(Clone, Copy)]
enum FakeProbe {
    Gone,
    Alive,
    Error,
}

struct FakeRuntime {
    probe: FakeProbe,
    missing_identity: bool,
}

impl loop_recovery::ResetRuntime for FakeRuntime {
    fn kill(&self, _work_dir: &Path, _agents: &[LiveAgent]) {}

    fn identity_missing(&self, _work_dir: &Path, _agent: &LiveAgent) -> bool {
        self.missing_identity
    }

    fn is_alive(&self, _work_dir: &Path, _agent: &LiveAgent) -> Result<bool> {
        match self.probe {
            FakeProbe::Gone => Ok(false),
            FakeProbe::Alive => Ok(true),
            FakeProbe::Error => bail!("injected probe failure"),
        }
    }

    fn wait(&self) {}
}

fn assert_reset_refused_without_mutation(work: &Path, session: &Session, error: anyhow::Error) {
    assert!(error.to_string().contains(&session.id));
    let stage = load_stage("alpha", work).unwrap();
    assert_eq!(stage.status, StageStatus::Executing);
    assert_eq!(stage.session.as_deref(), Some(session.id.as_str()));
    let persisted = load_session_exact(work, &session.id).unwrap().unwrap();
    assert_eq!(persisted.status, SessionStatus::Running);
    assert_eq!(persisted.exit_reason, None);
}

#[test]
fn live_agents_for_excludes_a_live_adjudication_session() {
    let temp = work_dir();
    let work = temp.path();

    let stage_session = session_for("alpha", SessionStatus::Running);
    spawn_a_live_agent(work, &stage_session);
    save_session(&stage_session, work).unwrap();

    let mut adjudication = Session::new_adjudication("alpha");
    adjudication.status = SessionStatus::Running;
    spawn_a_live_agent(work, &adjudication);
    save_session(&adjudication, work).unwrap();

    let agents = live_agents_for(work, "alpha").unwrap();
    assert_eq!(
        agents.len(),
        1,
        "the adjudication session must not count as a live agent"
    );
    match &agents[0] {
        LiveAgent::Known(session) => assert_eq!(session.id, stage_session.id),
        LiveAgent::Orphan(_) => panic!("expected the stage's own session, not an orphan"),
    }
}

#[test]
fn reset_refuses_a_surviving_target_without_mutating_ownership() {
    let temp = work_dir();
    let session = session_for("alpha", SessionStatus::Running);
    save_session(&session, temp.path()).unwrap();
    executing_stage(temp.path(), Some(&session));
    let runtime = FakeRuntime {
        probe: FakeProbe::Alive,
        missing_identity: false,
    };

    let error = loop_recovery::reset_with(temp.path(), "alpha", false, true, &runtime).unwrap_err();

    assert_reset_refused_without_mutation(temp.path(), &session, error);
}

#[test]
fn reset_refuses_a_probe_error_without_mutating_ownership() {
    let temp = work_dir();
    let session = session_for("alpha", SessionStatus::Running);
    save_session(&session, temp.path()).unwrap();
    executing_stage(temp.path(), Some(&session));
    let runtime = FakeRuntime {
        probe: FakeProbe::Error,
        missing_identity: false,
    };

    let error = loop_recovery::reset_with(temp.path(), "alpha", false, true, &runtime).unwrap_err();

    assert!(error.to_string().contains("probe error"));
    assert_reset_refused_without_mutation(temp.path(), &session, error);
}

#[test]
fn reset_refuses_a_tracked_session_without_pid_identity() {
    let temp = work_dir();
    let session = session_for("alpha", SessionStatus::Running);
    save_session(&session, temp.path()).unwrap();
    executing_stage(temp.path(), Some(&session));
    let runtime = FakeRuntime {
        probe: FakeProbe::Gone,
        missing_identity: true,
    };

    let error = loop_recovery::reset_with(temp.path(), "alpha", false, true, &runtime).unwrap_err();

    assert!(error.to_string().contains("PID identity unknown"));
    assert_reset_refused_without_mutation(temp.path(), &session, error);
}

#[test]
fn reset_records_operator_stop_only_after_all_targets_are_gone() {
    let temp = work_dir();
    let session = session_for("alpha", SessionStatus::Running);
    save_session(&session, temp.path()).unwrap();
    executing_stage(temp.path(), Some(&session));
    let runtime = FakeRuntime {
        probe: FakeProbe::Gone,
        missing_identity: false,
    };

    loop_recovery::reset_with(temp.path(), "alpha", false, true, &runtime).unwrap();

    let stage = load_stage("alpha", temp.path()).unwrap();
    assert_eq!(stage.status, StageStatus::WaitingForDeps);
    assert_eq!(stage.session, None);
    let persisted = load_session_exact(temp.path(), &session.id)
        .unwrap()
        .unwrap();
    assert_eq!(persisted.status, SessionStatus::ContextExhausted);
    assert_eq!(persisted.exit_reason, Some(SessionExitReason::OperatorStop));
}

#[test]
fn reset_without_live_agents_still_resets_the_stage() {
    let temp = work_dir();
    executing_stage(temp.path(), None);
    let runtime = FakeRuntime {
        probe: FakeProbe::Error,
        missing_identity: true,
    };

    loop_recovery::reset_with(temp.path(), "alpha", true, true, &runtime).unwrap();

    let stage = load_stage("alpha", temp.path()).unwrap();
    assert_eq!(stage.status, StageStatus::WaitingForDeps);
    assert_eq!(stage.session, None);
}
