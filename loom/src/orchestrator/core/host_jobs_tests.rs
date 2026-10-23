//! The host-job runner on the orchestrator tick: running, stopping, and
//! applying what ends, retries included.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use chrono::Utc;
use tempfile::TempDir;

use super::super::{Orchestrator, OrchestratorConfig};
use crate::fs::session_files::save_session;
use crate::fs::work_dir::write_terminal_config;
use crate::host_verify::runner::{HostJobRunner, RunnerConfig};
use crate::host_verify::store::{load_current, load_job_file};
use crate::host_verify::submit::{submit, Submission, SubmitRequest};
use crate::host_verify::{JobKind, JobRecord, JobState};
use crate::models::session::{Session, SessionBackendKind, SessionStatus, TerminalConfig};
use crate::models::stage::{Stage, StageStatus};
use crate::plan::ExecutionGraph;
use crate::verify::contracts::refusal::daemon_refusal;

const STAGE: &str = "stage-a";
/// Writes a passing `result.json` for the job the runner named.
const PASS: &str = r#"printf '{"job_id":"%s","head":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","passed":true,"criteria":[],"error":null}' "$3" > "$LOOM_WORK_DIR/verification/$1/$3/result.json""#;

/// An orchestrator over `<temp>/.loom/work` on the tmux lane (no terminal
/// detection), with stage-a's file in place for the runner.
fn orchestrator() -> (TempDir, PathBuf, Orchestrator) {
    let temp = TempDir::new().unwrap();
    let work = temp.path().join(".loom").join("work");
    std::fs::create_dir_all(&work).unwrap();
    let tmux = TerminalConfig {
        backend: SessionBackendKind::Tmux,
    };
    write_terminal_config(&work, &tmux).unwrap();
    let stage = Stage {
        id: STAGE.to_string(),
        name: STAGE.to_string(),
        status: StageStatus::Executing,
        ..Stage::default()
    };
    crate::verify::transitions::save_stage(&stage, &work).unwrap();
    let config = OrchestratorConfig {
        work_dir: work.clone(),
        repo_root: temp.path().to_path_buf(),
        enable_skill_routing: false,
        ..Default::default()
    };
    let graph = ExecutionGraph::build(Vec::new()).unwrap();
    let orchestrator = Orchestrator::new(config, graph).unwrap();
    (temp, work, orchestrator)
}

fn shell_runner(work: &Path, script: &str) -> HostJobRunner {
    let config = RunnerConfig {
        program: PathBuf::from("/bin/sh"),
        args_prefix: vec!["-c".into(), script.into(), "sh".into()],
        slots: 1,
    };
    HostJobRunner::new(work.to_path_buf(), config)
}

fn queue(work: &Path, kind: JobKind) -> JobRecord {
    let request = SubmitRequest {
        stage_id: STAGE,
        session_id: "session-a",
        request_id: "r1",
        kind,
        head: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        tree_digest: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        no_cache: false,
        criteria: None,
    };
    match submit(work, &request, Utc::now()).unwrap() {
        Submission::Queued(record) => record,
        other => panic!("expected a queued job, got {other:?}"),
    }
}

/// Tick until the current record satisfies `done`, at most 20 s.
fn tick_until(
    orchestrator: &mut Orchestrator,
    work: &Path,
    done: impl Fn(&JobRecord) -> bool,
) -> JobRecord {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(20) {
        orchestrator.tick_host_jobs();
        if let Some(record) = load_current(work, STAGE).unwrap().filter(&done) {
            return record;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("the host job never reached the expected state");
}

#[test]
fn the_tick_runs_a_queued_job_to_its_result() {
    let (_temp, work, mut orchestrator) = orchestrator();
    orchestrator.set_host_job_runner(shell_runner(&work, PASS));
    let job = queue(&work, JobKind::Check);

    let ended = tick_until(&mut orchestrator, &work, |r| r.state.is_terminal());

    assert_eq!(ended.id, job.id);
    assert_eq!(ended.state, JobState::Passed, "{ended:?}");
}

#[test]
fn stopping_the_host_jobs_cancels_a_running_job_and_kills_its_child() {
    let (_temp, work, mut orchestrator) = orchestrator();
    orchestrator.set_host_job_runner(shell_runner(&work, "sleep 600"));
    queue(&work, JobKind::Check);
    let running = tick_until(&mut orchestrator, &work, |r| {
        r.state == JobState::Running && r.pid.is_some()
    });

    orchestrator.stop_host_jobs();

    let stopped = load_current(&work, STAGE).unwrap().unwrap();
    assert_eq!(stopped.state, JobState::Cancelled);
    assert_eq!(stopped.detail.as_deref(), Some("daemon stopped"));
    let pid = nix::unistd::Pid::from_raw(i32::try_from(running.pid.unwrap()).unwrap());
    assert_eq!(
        nix::sys::signal::kill(pid, None),
        Err(nix::errno::Errno::ESRCH),
        "the stopped job's child must be killed and reaped"
    );
}

/// A freeze job that ends `Failed` records a refusal for stage-a's running
/// contract writer `session-a`; with the stage's contracts directory
/// read-only, every application of it fails. Returns the job's record once
/// the tick that ended it has run.
fn failing_freeze(work: &Path, orchestrator: &mut Orchestrator) -> JobRecord {
    let mut writer = Session::new_contract(STAGE);
    writer.id = "session-a".to_string();
    writer.status = SessionStatus::Running;
    save_session(&writer, work).unwrap();
    crate::verify::transitions::update_stage(STAGE, work, |stage| {
        stage.session = Some(writer.id.clone());
        Ok(())
    })
    .unwrap();
    orchestrator.set_host_job_runner(shell_runner(work, "exit 1"));
    let contracts = contracts_dir(work);
    std::fs::create_dir_all(&contracts).unwrap();
    std::fs::set_permissions(&contracts, std::fs::Permissions::from_mode(0o555)).unwrap();
    queue(work, JobKind::Freeze);
    tick_until(orchestrator, work, |r| r.state.is_terminal())
}

fn contracts_dir(work: &Path) -> PathBuf {
    work.join("contracts").join(STAGE)
}

fn set_writable(work: &Path) {
    let contracts = contracts_dir(work);
    std::fs::set_permissions(&contracts, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn marker(work: &Path, job: &JobRecord) -> Option<String> {
    load_job_file(work, STAGE, &job.id, "applied")
        .unwrap()
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

/// A root process writes through the read-only directory, so the failure
/// cannot be injected.
fn running_as_root() -> bool {
    // SAFETY: `geteuid` has no preconditions.
    unsafe { libc::geteuid() == 0 }
}

#[test]
fn an_application_that_fails_twice_is_applied_on_the_third_tick() {
    if running_as_root() {
        return;
    }
    let (_temp, work, mut orchestrator) = orchestrator();
    let job = failing_freeze(&work, &mut orchestrator);
    orchestrator.tick_host_jobs();
    assert_eq!(marker(&work, &job), None, "two failures leave it unmarked");
    assert_eq!(daemon_refusal(&work, STAGE).unwrap(), None);

    set_writable(&work);
    orchestrator.tick_host_jobs();

    assert!(
        marker(&work, &job).is_some(),
        "the third attempt applies it"
    );
    let stage = crate::verify::transitions::load_stage(STAGE, &work).unwrap();
    assert_eq!(
        stage.status,
        StageStatus::Executing,
        "a refusal parks nothing"
    );
    let refusal = daemon_refusal(&work, STAGE)
        .unwrap()
        .expect("the refusal is recorded");
    assert_eq!(refusal.session_id, "session-a");
}

#[test]
fn an_application_that_fails_ten_times_is_marked_failed() {
    if running_as_root() {
        return;
    }
    let (_temp, work, mut orchestrator) = orchestrator();
    let job = failing_freeze(&work, &mut orchestrator);
    for _ in 0..8 {
        orchestrator.tick_host_jobs();
    }
    assert_eq!(marker(&work, &job), None, "nine failures leave it unmarked");

    orchestrator.tick_host_jobs();
    set_writable(&work);

    let marked = marker(&work, &job).unwrap_or_default();
    assert!(marked.starts_with("apply failed: "), "{marked}");
    orchestrator.tick_host_jobs();
    let stage = crate::verify::transitions::load_stage(STAGE, &work).unwrap();
    assert_eq!(stage.status, StageStatus::Executing, "never applied again");
    assert_eq!(daemon_refusal(&work, STAGE).unwrap(), None);
}
