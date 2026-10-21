//! The host-job runner with `/bin/sh` standing in for `verify-host`: results,
//! failures, slots, deadline, cancellation, criterion groups, the child's
//! environment, and recovery after a restart.

use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use tempfile::TempDir;

use super::runner::{HostJobRunner, RunnerConfig};
use super::store::{append_group, load_current};
use super::submit::{cancel, submit, Submission, SubmitRequest};
use super::{JobKind, JobRecord, JobState};
use crate::models::stage::{Stage, StageStatus};

pub(super) const STAGE: &str = "stage-a";
/// Writes a passing `result.json` for the job the runner named.
pub(super) const PASS: &str = r#"printf '{"job_id":"%s","head":"h","passed":true,"criteria":[],"error":null}' "$3" > "$LOOM_WORK_DIR/verification/$1/$3/result.json""#;

pub(super) fn write_stage(work: &Path, stage_id: &str) {
    let stage = Stage {
        id: stage_id.to_string(),
        name: stage_id.to_string(),
        status: StageStatus::Executing,
        ..Stage::default()
    };
    crate::verify::transitions::save_stage(&stage, work).unwrap();
}

/// `<temp>/.loom/work` holding stage-a's file.
pub(super) fn state_root() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let work = dir.path().join(".loom").join("work");
    std::fs::create_dir_all(&work).unwrap();
    write_stage(&work, STAGE);
    (dir, work)
}

pub(super) fn shell(script: &str, slots: usize) -> RunnerConfig {
    RunnerConfig {
        program: PathBuf::from("/bin/sh"),
        args_prefix: vec!["-c".into(), script.into(), "sh".into()],
        slots,
    }
}

pub(super) fn queue(work: &Path, stage_id: &str, at: DateTime<Utc>) -> JobRecord {
    let request = SubmitRequest {
        stage_id,
        session_id: "session-a",
        request_id: "r1",
        kind: JobKind::Check,
        head: "h",
        tree_digest: "d",
        no_cache: false,
    };
    match submit(work, &request, at).unwrap() {
        Submission::Queued(record) => record,
        other => panic!("expected a queued job, got {other:?}"),
    }
}

/// Tick until the stage's current record satisfies `done`, at most 20 s.
pub(super) fn tick_until(
    runner: &mut HostJobRunner,
    work: &Path,
    stage_id: &str,
    done: impl Fn(&JobRecord) -> bool,
) -> JobRecord {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(20) {
        runner.tick().unwrap();
        if let Some(record) = load_current(work, stage_id).unwrap().filter(&done) {
            return record;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("stage {stage_id}'s job never reached the expected state");
}

pub(super) fn running(record: &JobRecord) -> bool {
    record.state == JobState::Running && record.pid.is_some()
}

/// Whether `pid` names no process at all (a zombie still counts).
fn gone(pid: u32) -> bool {
    let pid = nix::unistd::Pid::from_raw(i32::try_from(pid).unwrap());
    nix::sys::signal::kill(pid, None) == Err(nix::errno::Errno::ESRCH)
}

/// A stand-in criterion leading its own process group, as a confined one
/// does; killed and reaped on drop so a failing test leaks nothing.
pub(super) struct Criterion(pub(super) Child);

impl Criterion {
    pub(super) fn spawn() -> Self {
        let child = Command::new("sleep")
            .arg("600")
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        Self(child)
    }

    /// Whether it was killed by SIGKILL within 5 s.
    pub(super) fn killed(&mut self) -> bool {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(5) {
            if let Some(status) = self.0.try_wait().unwrap() {
                return status.signal() == Some(libc::SIGKILL);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    }
}

impl Drop for Criterion {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn a_child_that_writes_a_passing_result_turns_passed() {
    let (_dir, work) = state_root();
    let mut runner = HostJobRunner::new(work.clone(), shell(PASS, 1));
    let job = queue(&work, STAGE, Utc::now());

    let ended = tick_until(&mut runner, &work, STAGE, |r| r.state.is_terminal());

    assert_eq!(ended.id, job.id);
    assert_eq!(ended.state, JobState::Passed, "{ended:?}");
    assert!(ended.started_at.is_some() && ended.finished_at.is_some());
}

#[test]
fn a_child_that_exits_without_a_result_fails_with_its_log_tail() {
    let (_dir, work) = state_root();
    let script = r#"i=0; while [ $i -lt 200 ]; do echo "line $i"; i=$((i+1)); done; exit 1"#;
    let mut runner = HostJobRunner::new(work.clone(), shell(script, 1));
    queue(&work, STAGE, Utc::now());

    let ended = tick_until(&mut runner, &work, STAGE, |r| r.state.is_terminal());

    assert_eq!(ended.state, JobState::Failed);
    let detail = ended.detail.unwrap_or_default();
    assert!(
        detail.starts_with("verify-host exited 1 without a result"),
        "{detail}"
    );
    assert!(detail.ends_with("line 198\nline 199"), "{detail}");
    assert!(detail.len() <= 512, "{} bytes", detail.len());
}

#[test]
fn a_job_whose_stage_cannot_be_loaded_fails_without_starting() {
    let (_dir, work) = state_root();
    let mut runner = HostJobRunner::new(work.clone(), shell(PASS, 1));
    queue(&work, "no-such-stage", Utc::now());

    let ended = runner.tick().unwrap();

    assert_eq!(ended.len(), 1, "{ended:?}");
    assert_eq!(ended[0].state, JobState::Failed);
    assert_eq!(ended[0].pid, None);
    let detail = ended[0].detail.clone().unwrap_or_default();
    assert!(
        detail.contains("cannot load stage 'no-such-stage'"),
        "{detail}"
    );
}

#[test]
fn one_slot_runs_queued_jobs_one_after_the_other() {
    let (_dir, work) = state_root();
    write_stage(&work, "stage-b");
    let mut runner = HostJobRunner::new(work.clone(), shell("sleep 0.3", 1));
    queue(&work, STAGE, Utc::now() - chrono::Duration::seconds(1));
    queue(&work, "stage-b", Utc::now());

    let start = Instant::now();
    let (first, second) = loop {
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "the jobs never ended"
        );
        runner.tick().unwrap();
        let a = load_current(&work, STAGE).unwrap().unwrap();
        let b = load_current(&work, "stage-b").unwrap().unwrap();
        assert!(
            !(a.state == JobState::Running && b.state == JobState::Running),
            "two jobs ran in one slot"
        );
        if a.state.is_terminal() && b.state.is_terminal() {
            break (a, b);
        }
        std::thread::sleep(Duration::from_millis(50));
    };

    assert!(second.started_at.unwrap() >= first.finished_at.unwrap());
}

#[test]
fn a_job_past_its_deadline_fails_and_its_child_and_criteria_die() {
    let (_dir, work) = state_root();
    let mut runner = HostJobRunner::new(work.clone(), shell("sleep 600", 1))
        .with_deadline_override(Duration::from_secs(1));
    let job = queue(&work, STAGE, Utc::now());
    let started = tick_until(&mut runner, &work, STAGE, running);
    let mut criterion = Criterion::spawn();
    append_group(&work, STAGE, &job.id, criterion.0.id()).unwrap();
    std::thread::sleep(Duration::from_millis(1_200));

    let ended = runner.tick().unwrap();

    let failed = ended
        .iter()
        .find(|r| r.id == job.id)
        .expect("the job ended");
    assert_eq!(failed.state, JobState::Failed);
    assert_eq!(failed.detail.as_deref(), Some("deadline of 1 s passed"));
    assert!(
        gone(started.pid.unwrap()),
        "the child was not killed and reaped"
    );
    assert!(
        criterion.killed(),
        "the job's criterion outlived its deadline"
    );
}

#[test]
fn cancelling_a_running_job_kills_its_child_and_criteria() {
    let (_dir, work) = state_root();
    let mut runner = HostJobRunner::new(work.clone(), shell("sleep 600", 1));
    let job = queue(&work, STAGE, Utc::now());
    let started = tick_until(&mut runner, &work, STAGE, running);
    let mut criterion = Criterion::spawn();
    append_group(&work, STAGE, &job.id, criterion.0.id()).unwrap();

    cancel(&work, STAGE, "takedown").unwrap();
    let ended = runner.tick().unwrap();

    assert!(ended
        .iter()
        .any(|r| r.id == job.id && r.state == JobState::Cancelled));
    assert!(
        gone(started.pid.unwrap()),
        "the child was not killed and reaped"
    );
    assert!(
        criterion.killed(),
        "the job's criterion outlived the cancel"
    );
}

#[test]
fn a_record_cancelled_while_its_child_runs_stays_cancelled_after_the_reap() {
    let (_dir, work) = state_root();
    let script = format!("sleep 0.2; {PASS}");
    let mut runner = HostJobRunner::new(work.clone(), shell(&script, 1));
    let job = queue(&work, STAGE, Utc::now());
    tick_until(&mut runner, &work, STAGE, running);

    cancel(&work, STAGE, "operator stop").unwrap();
    std::thread::sleep(Duration::from_millis(600));
    let ended = runner.tick().unwrap();

    let current = load_current(&work, STAGE).unwrap().unwrap();
    assert_eq!(current.state, JobState::Cancelled);
    assert_eq!(current.detail.as_deref(), Some("operator stop"));
    assert!(ended
        .iter()
        .any(|r| r.id == job.id && r.state == JobState::Cancelled));
}

#[test]
fn a_failed_start_pass_still_returns_the_records_already_settled() {
    use std::os::unix::fs::PermissionsExt;
    // SAFETY: `geteuid` has no preconditions.
    if unsafe { libc::geteuid() } == 0 {
        return; // root lists any directory, so the failure cannot be injected
    }
    let (_dir, work) = state_root();
    let mut runner = HostJobRunner::new(work.clone(), shell("sleep 30", 1));
    let job = queue(&work, STAGE, Utc::now());
    tick_until(&mut runner, &work, STAGE, running);
    cancel(&work, STAGE, "operator stop").unwrap();
    // Traversable but not listable: `start_queued` fails listing the stages,
    // while the cancelled record is still readable.
    let verification = work.join("verification");
    std::fs::set_permissions(&verification, std::fs::Permissions::from_mode(0o111)).unwrap();

    // Every tick runs the failing start pass; under load the cancelled child
    // can take more than one tick to be reaped, so tick until it is returned.
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let returned = loop {
        let found = runner
            .tick()
            .map(|ended| {
                ended
                    .iter()
                    .any(|r| r.id == job.id && r.state == JobState::Cancelled)
            })
            .map_err(|err| format!("{err:#}"));
        if !matches!(found, Ok(false)) || std::time::Instant::now() > deadline {
            break found;
        }
        std::thread::sleep(Duration::from_millis(100));
    };

    std::fs::set_permissions(&verification, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(returned.unwrap(), "the cancelled record was never returned");
}
