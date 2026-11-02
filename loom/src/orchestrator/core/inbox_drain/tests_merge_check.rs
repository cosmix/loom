//! `merge-resolved` and a resolver's exit beside the check a resolver
//! requested: a live check is stopped before the gates run, and a passed one
//! lands nothing.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use chrono::Utc;

use crate::fs::inbox::LedgerOutcome;
use crate::host_verify::runner::{HostJobRunner, RunnerConfig};
use crate::host_verify::store::{load_current, save_current, save_result};
use crate::host_verify::submit::{submit, Submission, SubmitRequest};
use crate::host_verify::{JobKind, JobRecord, JobResult, JobState};
use crate::models::stage::{AcceptanceCriterion, Stage, StageStatus};
use crate::orchestrator::core::Orchestrator;
use crate::verify::transitions::{load_stage, save_stage, update_stage};

use super::test_support::{fixture, Fixture, STAGE};
use super::tests_merge::{git, resolve, resolved_stage, worktree_of};

/// Where the fake check's child leaves its heartbeat: ignored by the fixture's
/// `.gitignore` and by completion's snapshot, so the worktree stays clean.
const HEARTBEAT: &str = ".loom/cache/heartbeat";

/// Queue a check of the worktree as it stands, the way the relay does.
fn queue_check(fx: &Fixture) -> JobRecord {
    queue_check_of(fx, STAGE)
}

fn queue_check_of(fx: &Fixture, stage_id: &str) -> JobRecord {
    let head = git(&worktree_of(fx), &["rev-parse", "HEAD"]);
    let digest = "0".repeat(64);
    let request = SubmitRequest {
        stage_id,
        session_id: "resolver",
        request_id: "r1",
        kind: JobKind::Check,
        head: &head,
        tree_digest: &digest,
        no_cache: false,
        criteria: None,
    };
    match submit(&fx.work_dir, &request, Utc::now()).unwrap() {
        Submission::Queued(job) => job,
        other => panic!("expected a queued check, got {other:?}"),
    }
}

/// A runner of one job at a time whose child appends to `heartbeat` until it
/// is killed.
fn heartbeat_runner(work_dir: &Path, heartbeat: &Path) -> HostJobRunner {
    let script = format!(
        "while :; do echo x >> '{}'; sleep 0.02; done",
        heartbeat.display()
    );
    let config = RunnerConfig {
        program: PathBuf::from("/bin/sh"),
        args_prefix: vec!["-c".into(), script, "sh".into()],
        slots: 1,
    };
    HostJobRunner::new(work_dir.to_path_buf(), config)
}

/// The fixture's resolved stage with a check whose child is running, and a
/// criterion that fails while that child still writes: the gate itself sees
/// whether the child outlived the stop. The criterion runs in a PID namespace
/// of its own, so the heartbeat is what reaches it, and a pid would not.
fn stage_with_running_check(fx: &Fixture) -> (Orchestrator, JobRecord) {
    let mut orchestrator = resolved_stage(fx);
    let heartbeat = worktree_of(fx).join(HEARTBEAT);
    std::fs::create_dir_all(heartbeat.parent().unwrap()).unwrap();
    update_stage(STAGE, &fx.work_dir, |stage| {
        let size = format!("wc -c < '{}'", heartbeat.display());
        let quiet =
            format!("a=$({size}); test -n \"$a\" && sleep 0.5; b=$({size}); test \"$a\" = \"$b\"");
        stage.acceptance = vec![AcceptanceCriterion::Simple(quiet)];
        Ok(())
    })
    .unwrap();
    orchestrator.set_host_job_runner(heartbeat_runner(&fx.work_dir, &heartbeat));
    let job = queue_check(fx);
    let running = tick_until_child_writes(fx, &mut orchestrator, &heartbeat);
    assert_eq!(running.id, job.id);
    (orchestrator, running)
}

fn tick_until_child_writes(
    fx: &Fixture,
    orchestrator: &mut Orchestrator,
    heartbeat: &Path,
) -> JobRecord {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(20) {
        orchestrator.tick_host_jobs();
        let current = load_current(&fx.work_dir, STAGE).unwrap().unwrap();
        if current.state == JobState::Running && current.pid.is_some() && heartbeat.exists() {
            return current;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("the check's child never started writing");
}

fn assert_stopped(fx: &Fixture, running: &JobRecord) {
    let current = load_current(&fx.work_dir, STAGE).unwrap().unwrap();
    assert_eq!(current.id, running.id);
    assert_eq!(current.state, JobState::Cancelled);
    let pid = nix::unistd::Pid::from_raw(i32::try_from(running.pid.unwrap()).unwrap());
    assert_eq!(
        nix::sys::signal::kill(pid, None),
        Err(nix::errno::Errno::ESRCH),
        "the stopped check's child must be killed and reaped"
    );
}

#[test]
fn merge_resolved_kills_a_running_check_before_the_gates_run() {
    let fx = fixture();
    let (mut orchestrator, running) = stage_with_running_check(&fx);

    assert_eq!(
        resolve(&fx, &mut orchestrator),
        Some(LedgerOutcome::Applied)
    );

    assert_stopped(&fx, &running);
    assert!(
        load_stage(STAGE, &fx.work_dir).unwrap().merged,
        "a child alive at the gates fails the quiet criterion"
    );
}

#[test]
fn a_resolver_exit_kills_a_running_check_before_the_gates_run() {
    let fx = fixture();
    let (mut orchestrator, running) = stage_with_running_check(&fx);

    orchestrator
        .handle_merge_session_completed("resolver", STAGE)
        .unwrap();

    assert_stopped(&fx, &running);
    assert!(
        load_stage(STAGE, &fx.work_dir).unwrap().merged,
        "a child alive at the gates fails the quiet criterion"
    );
}

/// The gates block the tick, so a job started just before them would spend its
/// deadline waiting for the next tick to look at it.
#[test]
fn stopping_the_check_starts_no_queued_job_of_another_stage() {
    let fx = fixture();
    let (mut orchestrator, _running) = stage_with_running_check(&fx);
    let other = Stage {
        id: "other".to_string(),
        name: "other".to_string(),
        status: StageStatus::Executing,
        ..Stage::default()
    };
    save_stage(&other, &fx.work_dir).unwrap();
    let queued = queue_check_of(&fx, "other");

    assert_eq!(
        resolve(&fx, &mut orchestrator),
        Some(LedgerOutcome::Applied)
    );

    let current = load_current(&fx.work_dir, "other").unwrap().unwrap();
    assert_eq!(current.id, queued.id);
    assert_eq!(current.state, JobState::Queued);
}

#[test]
fn merge_resolved_cancels_a_queued_check_no_child_runs_for() {
    let fx = fixture();
    let mut orchestrator = resolved_stage(&fx);
    let job = queue_check(&fx);

    assert_eq!(
        resolve(&fx, &mut orchestrator),
        Some(LedgerOutcome::Applied)
    );

    let current = load_current(&fx.work_dir, STAGE).unwrap().unwrap();
    assert_eq!(current.id, job.id);
    assert_eq!(current.state, JobState::Cancelled);
    assert!(load_stage(STAGE, &fx.work_dir).unwrap().merged);
}

fn assert_job_untouched(fx: &Fixture, job: &JobRecord) {
    let current = load_current(&fx.work_dir, STAGE).unwrap().unwrap();
    assert_eq!(current.id, job.id);
    assert_eq!(current.state, JobState::Queued);
}

#[test]
fn a_stage_outside_its_merge_state_refuses_and_leaves_its_job_alone() {
    let fx = fixture();
    let mut orchestrator = resolved_stage(&fx);
    let job = queue_check(&fx);
    update_stage(STAGE, &fx.work_dir, |stage| {
        stage.status = StageStatus::Executing;
        Ok(())
    })
    .unwrap();

    assert_eq!(
        resolve(&fx, &mut orchestrator),
        Some(LedgerOutcome::Refused)
    );

    assert_job_untouched(&fx, &job);
}

/// A resolver told to finish its worktree keeps the check it is still reading.
#[test]
fn a_dirty_worktree_refuses_and_leaves_the_stages_job_alone() {
    let fx = fixture();
    let mut orchestrator = resolved_stage(&fx);
    let job = queue_check(&fx);
    std::fs::write(worktree_of(&fx).join("b.txt"), "changed\n").unwrap();

    assert_eq!(
        resolve(&fx, &mut orchestrator),
        Some(LedgerOutcome::Refused)
    );

    assert_job_untouched(&fx, &job);
}

#[test]
fn a_passed_check_does_not_stand_in_for_the_landing_gates() {
    let fx = fixture();
    let mut orchestrator = resolved_stage(&fx);
    let mut job = queue_check(&fx);
    job.state = JobState::Passed;
    job.finished_at = Some(Utc::now());
    save_current(&fx.work_dir, &job).unwrap();
    let result = JobResult {
        job_id: job.id.clone(),
        head: job.head.clone(),
        passed: true,
        criteria: Vec::new(),
        error: None,
        policy_hash: None,
    };
    save_result(&fx.work_dir, STAGE, &result).unwrap();
    update_stage(STAGE, &fx.work_dir, |stage| {
        stage.acceptance = vec![AcceptanceCriterion::Simple("false".into())];
        Ok(())
    })
    .unwrap();
    let main = git(&fx.repo_root, &["rev-parse", "main"]);

    resolve(&fx, &mut orchestrator);

    assert!(!load_stage(STAGE, &fx.work_dir).unwrap().merged);
    assert_eq!(git(&fx.repo_root, &["rev-parse", "main"]), main);
}
