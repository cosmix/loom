//! Runner recovery after a restart, and the environment the child runs in.

use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use chrono::Utc;
use serial_test::serial;
use tempfile::TempDir;

use super::runner::{
    budget_for_job, installed_binary, job_deadline_secs, recover_after_restart, HostJobRunner,
};
use super::spawn::is_session_variable;
use super::store::{append_group, load_current, save_current};
use super::tests_complete_support::Fixture;
use super::tests_runner::{
    guarded_criterion, queue, shell, state_root, tick_until, write_stage, Criterion, PASS, STAGE,
};
use super::{new_job_id, JobKind, JobRecord, JobState};
use crate::models::stage::StageType;
use crate::models::stage::{AcceptanceCriterion, Stage, TruthCheck};

/// A session-prefixed name no other code reads, so the tests running beside
/// this one never see it.
const PROBE: &str = "LOOM_SESSION_RUNNER_PROBE";

/// Removes [`PROBE`] again, even when the test fails.
struct ProbeSet;

impl Drop for ProbeSet {
    fn drop(&mut self) {
        // SAFETY: serialized test; nothing else reads or writes this name.
        unsafe { std::env::remove_var(PROBE) };
    }
}

#[test]
#[serial]
fn the_child_runs_from_the_project_root_with_no_session_variables() {
    let dir = TempDir::new().unwrap();
    let project = dir.path().join("project");
    let work = project.join(".loom").join("work");
    std::fs::create_dir_all(&work).unwrap();
    write_stage(&work, STAGE);
    let linked = dir.path().join("linked-work");
    std::os::unix::fs::symlink(&work, &linked).unwrap();
    let (root, state) = (
        project.canonicalize().unwrap(),
        work.canonicalize().unwrap(),
    );
    // SAFETY: serialized test; nothing else reads or writes this name.
    unsafe { std::env::set_var(PROBE, "1") };
    let _probe = ProbeSet;
    let script = format!(
        "[ -z \"${{{PROBE}+x}}\" ] && [ \"$LOOM_WORK_DIR\" = '{}' ] \
         && [ \"$(pwd -P)\" = '{}' ] && {PASS}",
        state.display(),
        root.display()
    );
    let mut runner = HostJobRunner::new(linked.clone(), shell(&script, 1));
    queue(&linked, STAGE, Utc::now());

    let ended = tick_until(&mut runner, &linked, STAGE, |r| r.state.is_terminal());

    assert_eq!(ended.state, JobState::Passed, "{ended:?}");
}

#[test]
fn every_session_variable_is_kept_from_the_child() {
    for key in [
        "LOOM_SESSION_ID",
        "LOOM_SESSION_TYPE",
        "LOOM_STAGE_ID",
        "LOOM_SCRATCH_DIR",
        "LOOM_WORKTREE_PATH",
        "LOOM_HOOK_CONTEXT",
        "LOOM_MAIN_AGENT_PID",
        "LOOM_MERGE_SESSION",
        "LOOM_BIN",
        "LOOM_HOOK_PATH",
    ] {
        assert!(is_session_variable(key), "{key}");
    }
    for key in ["LOOM_WORK_DIR", "LOOM_HOME", "PATH", "HOME"] {
        assert!(!is_session_variable(key), "{key}");
    }
}

#[test]
fn the_deadline_counts_a_criterion_at_its_own_timeout() {
    let long = TruthCheck {
        command: "cargo test".to_string(),
        stdout_contains: Vec::new(),
        stdout_not_contains: Vec::new(),
        stderr_empty: None,
        exit_code: None,
        timeout_secs: Some(3_600),
        description: None,
    };
    let stage = Stage {
        acceptance: vec![
            AcceptanceCriterion::Extended(long),
            AcceptanceCriterion::Simple("true".to_string()),
        ],
        ..Stage::default()
    };

    // 3600 + 300 for the simple criterion + 300 for the impact selection +
    // the 1800 s completion margin.
    assert_eq!(job_deadline_secs(&stage), 6_000);
}

#[test]
fn the_deadline_counts_each_after_stage_timeout_and_saturates() {
    let check = |timeout_secs| TruthCheck {
        command: "true".to_string(),
        stdout_contains: Vec::new(),
        stdout_not_contains: Vec::new(),
        stderr_empty: None,
        exit_code: None,
        timeout_secs,
        description: None,
    };
    let mut stage = Stage {
        after_stage: vec![check(Some(3_600)), check(Some(42)), check(None)],
        ..Stage::default()
    };
    assert_eq!(job_deadline_secs(&stage), 300 + 1_800 + 3_600 + 42 + 30);
    stage.after_stage.push(check(Some(u64::MAX)));
    assert_eq!(job_deadline_secs(&stage), u64::MAX);

    let default_acceptance = Stage {
        acceptance: vec![AcceptanceCriterion::Extended(check(None))],
        ..Stage::default()
    };
    assert_eq!(job_deadline_secs(&default_acceptance), 30 + 300 + 1_800);
}

#[test]
fn completion_budget_uses_the_executable_plan_and_binds_the_child() {
    let fx = Fixture::new(StageType::Standard, 1);
    let plan = fx.work_dir.join("PLAN-host.md");
    let original = std::fs::read_to_string(&plan).unwrap();
    let long = original.replace(
        "        - \"true\"\n",
        "        - \"true\"\n      wiring_tests:\n        - name: \"plan-only wiring\"\n          command: \"true\"\n      after_stage:\n        - command: \"true\"\n          timeout_secs: 3600\n",
    );
    assert_ne!(original, long);
    std::fs::write(&plan, &long).unwrap();
    let record = fx.record(JobKind::Complete, JobState::Running);
    let (deadline, policy) = budget_for_job(&record, &fx.work_dir).unwrap();
    assert_eq!(deadline, Duration::from_secs(6_300));
    let mut check = record.clone();
    check.kind = JobKind::Check;
    assert_eq!(
        budget_for_job(&check, &fx.work_dir).unwrap().0,
        Duration::from_secs(2_700)
    );

    let changed = long.replace("timeout_secs: 3600", "timeout_secs: 42");
    std::fs::write(&plan, changed).unwrap();
    let error = super::child_policy::run_job_bound(&fx.work_dir, STAGE, &record.id, &policy)
        .expect_err("a changed executable plan must be refused before checks");
    assert!(error.to_string().contains("policy changed"), "{error:#}");
    assert!(super::store::load_result(&fx.work_dir, STAGE, &record.id)
        .unwrap()
        .is_none());
}

pub(super) fn running_record(stage_id: &str, pid: Option<u32>) -> JobRecord {
    JobRecord {
        id: new_job_id(),
        submission_order: 0,
        stage_id: stage_id.to_string(),
        session_id: "session-a".to_string(),
        kind: JobKind::Check,
        head: "h".to_string(),
        tree_digest: "d".to_string(),
        no_cache: false,
        criteria: None,
        state: JobState::Running,
        requests: vec!["r1".to_string()],
        queued_at: Utc::now(),
        started_at: Some(Utc::now()),
        finished_at: None,
        pid,
        detail: None,
    }
}

#[test]
fn recovery_interrupts_running_jobs_and_leaves_queued_ones_alone() {
    let (_dir, work) = state_root();
    // A live process that is not this job's child: recovery must not signal it.
    let mut stranger = Criterion::spawn();
    let interrupted = running_record(STAGE, Some(stranger.0.id()));
    save_current(&work, &interrupted).unwrap();
    let (_anchor, mut criterion) = guarded_criterion(&work, &interrupted);
    let waiting = queue(&work, "stage-b", Utc::now());

    let changed = recover_after_restart(&work).unwrap();

    assert_eq!(changed.len(), 1, "{changed:?}");
    assert_eq!(changed[0].id, interrupted.id);
    let current = load_current(&work, STAGE).unwrap().unwrap();
    assert_eq!(current.state, JobState::Interrupted);
    assert_eq!(
        current.detail.as_deref(),
        Some("daemon restarted during the run")
    );
    assert_eq!(load_current(&work, "stage-b").unwrap(), Some(waiting));
    assert!(criterion.killed(), "a guarded criterion survived recovery");
    assert!(
        stranger.0.try_wait().unwrap().is_none(),
        "recovery signalled a pid whose command line names no job"
    );
}

#[test]
fn recovery_does_not_signal_a_reused_or_legacy_group() {
    let (_dir, work) = state_root();
    let record = running_record(STAGE, None);
    save_current(&work, &record).unwrap();
    let mut reused = Criterion::spawn();
    let mut legacy = Criterion::spawn();
    let start = crate::process::process_start_time(reused.0.id()).unwrap();
    let boot = crate::process::boot_id::os_boot_id().unwrap();
    let groups = super::store::job_dir(&work, STAGE, &record.id)
        .unwrap()
        .join("groups");
    std::fs::write(
        groups,
        format!(
            "{} {} {}\n{}\n",
            reused.0.id(),
            start.wrapping_add(1),
            boot,
            legacy.0.id()
        ),
    )
    .unwrap();

    recover_after_restart(&work).unwrap();

    assert!(reused.0.try_wait().unwrap().is_none());
    assert!(legacy.0.try_wait().unwrap().is_none());
}

#[test]
fn recovery_retries_an_interruption_written_before_cleanup() {
    let (_dir, work) = state_root();
    let mut record = running_record(STAGE, None);
    record.state = JobState::Interrupted;
    record.finished_at = Some(Utc::now());
    save_current(&work, &record).unwrap();
    let (_anchor, mut criterion) = guarded_criterion(&work, &record);

    let changed = recover_after_restart(&work).unwrap();

    assert!(changed.is_empty(), "the interruption was already recorded");
    assert!(
        criterion.killed(),
        "recovery skipped the interrupted job's group"
    );
}

#[test]
fn recovery_retires_a_superseded_jobs_guardian() {
    let (_dir, work) = state_root();
    let mut old = running_record(STAGE, None);
    save_current(&work, &old).unwrap();
    let (_anchor, mut criterion) = guarded_criterion(&work, &old);
    old.state = JobState::Cancelled;
    old.finished_at = Some(Utc::now());
    save_current(&work, &old).unwrap();
    let mut queued = running_record(STAGE, None);
    queued.state = JobState::Queued;
    queued.started_at = None;
    save_current(&work, &queued).unwrap();

    let changed = recover_after_restart(&work).unwrap();

    assert!(changed.is_empty());
    assert_eq!(load_current(&work, STAGE).unwrap(), Some(queued));
    assert!(
        criterion.killed(),
        "superseded job's criterion survived recovery"
    );
}

/// A stand-in verify-host child with the same state-triggered self-stop.
fn job_child(work: &std::path::Path, record: &JobRecord) -> Criterion {
    let marker = format!("verify-host {} --job {}", record.stage_id, record.id);
    let current = super::store::stage_dir(work, &record.stage_id)
        .unwrap()
        .join("current.json");
    let child = Command::new("/bin/sh")
        .args(["-c", "while grep -Eq '\"state\"[[:space:]]*:[[:space:]]*\"running\"' \"$1\"; do sleep 0.05; done; kill -s KILL 0", &marker])
        .arg(current)
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    Criterion(child)
}

/// Whether `child` exited within 5 s.
fn exits(child: &mut Child) -> bool {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(5) {
        if child.try_wait().unwrap().is_some() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

fn cancelled(record: &mut JobRecord, pid: u32) {
    record.pid = Some(pid);
    record.state = JobState::Cancelled;
    record.finished_at = Some(Utc::now());
}

#[test]
fn recovery_stops_a_job_cancelled_while_no_daemon_ran() {
    let (_dir, work) = state_root();
    let mut record = running_record(STAGE, None);
    save_current(&work, &record).unwrap();
    let mut child = job_child(&work, &record);
    cancelled(&mut record, child.0.id());
    save_current(&work, &record).unwrap();
    let (_anchor, mut criterion) = guarded_criterion(&work, &record);

    let changed = recover_after_restart(&work).unwrap();

    assert!(changed.is_empty(), "{changed:?}");
    assert_eq!(load_current(&work, STAGE).unwrap(), Some(record));
    assert!(exits(&mut child.0), "the cancelled job's child survived");
    assert!(
        criterion.killed(),
        "a criterion of the cancelled job survived"
    );
}

#[test]
fn recovery_leaves_the_groups_of_a_retired_cancelled_job_alone() {
    let (_dir, work) = state_root();
    let mut stranger = Criterion::spawn();
    let mut record = running_record(STAGE, None);
    cancelled(&mut record, stranger.0.id());
    save_current(&work, &record).unwrap();
    let mut reused = Criterion::spawn();
    append_group(&work, STAGE, &record.id, reused.0.id()).unwrap();

    recover_after_restart(&work).unwrap();

    assert!(
        reused.0.try_wait().unwrap().is_none(),
        "recovery signalled a group of a job whose child is gone"
    );
    assert!(stranger.0.try_wait().unwrap().is_none());
}

#[test]
fn a_replaced_binary_is_spawned_from_its_installed_path() {
    let dir = TempDir::new().unwrap();
    let installed = dir.path().join("loom");
    std::fs::write(&installed, b"").unwrap();

    assert_eq!(
        installed_binary(dir.path().join("loom (deleted)")),
        installed
    );
    let gone = dir.path().join("gone (deleted)");
    assert_eq!(installed_binary(gone.clone()), gone);
}
