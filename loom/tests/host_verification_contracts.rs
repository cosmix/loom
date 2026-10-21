//! Frozen contracts for stage host-jobs: an in-session `loom check` relays a
//! check ticket and runs nothing; the daemon's host-job store joins identical
//! requests, supersedes stale ones and refuses unsafe paths; the runner kills
//! cancelled and overdue jobs together with their criteria and interrupts
//! running jobs after a restart; `loom stage verify-host` records every
//! criterion and refuses to run inside a session; judges may relay `check`.

#[path = "integration/helpers.rs"]
#[allow(dead_code)]
mod helpers;
#[path = "host_verification_contracts/support.rs"]
mod support;

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use chrono::Utc;
use loom::host_verify::digest::tree_digest;
use loom::host_verify::runner::{recover_after_restart, HostJobRunner, RunnerConfig};
use loom::host_verify::store::{job_dir, load_current, load_groups, save_current, stage_dir};
use loom::host_verify::submit::{cancel, submit, Submission, SubmitRequest};
use loom::host_verify::{JobKind, JobRecord, JobState};
use loom::models::session::SessionType;
use loom::process::sandbox_probe::{confined_commands_runnable, skip_unless};
use loom::relay::{verdict, MatrixVerdict, RequestKind};
use serial_test::serial;
use tempfile::TempDir;

use support::{
    alive, build_fixture, record, state_root, verify_host_runner, wait_until, write_stage, STAGE,
};

const HEAD_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const HEAD_B: &str = "cccccccccccccccccccccccccccccccccccccccc";
const DIGEST: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn check_request<'a>(request_id: &'a str, head: &'a str, digest: &'a str) -> SubmitRequest<'a> {
    SubmitRequest {
        stage_id: STAGE,
        session_id: support::SESSION,
        request_id,
        kind: JobKind::Check,
        head,
        tree_digest: digest,
        no_cache: false,
    }
}

fn queued(submission: Submission) -> JobRecord {
    match submission {
        Submission::Queued(record) => record,
        other => panic!("expected Submission::Queued, got {other:?}"),
    }
}

fn sleeper() -> RunnerConfig {
    RunnerConfig {
        program: PathBuf::from("/bin/sh"),
        args_prefix: vec!["-c".into(), "sleep 600".into(), "sh".into()],
        slots: 1,
    }
}

#[test]
#[serial]
fn in_session_check_relays_and_runs_nothing() {
    let fx = build_fixture(&["touch ran-in-session"]);
    let output = fx.session_loom().args(["check", STAGE]).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{stdout}\n{stderr}");
    let first = stdout.lines().next().unwrap_or_default();
    assert!(
        first.starts_with("Host job: requested (check, "),
        "{stdout}"
    );
    let last = stdout.lines().next_back().unwrap_or_default();
    assert!(last.starts_with("LOOM_RELAY_V1"), "{stdout}");

    let tickets: Vec<PathBuf> = fs::read_dir(&fx.scratch_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_file() && path.extension().is_some_and(|ext| ext == "req"))
        .collect();
    assert_eq!(tickets.len(), 1, "{tickets:?}");
    let ticket: serde_json::Value =
        serde_json::from_slice(&fs::read(&tickets[0]).unwrap()).unwrap();
    assert_eq!(ticket["kind"], "check", "{ticket}");

    assert!(
        !fx.worktree.join("ran-in-session").exists(),
        "criteria ran in the session"
    );
    assert!(!fx.work_dir.join("verification").exists());
    assert!(!fx.work_dir.join("acceptance-cache").exists());
}

#[test]
fn same_request_joins_one_job() {
    let (_dir, w) = state_root();
    let now = Utc::now();
    let first = queued(submit(&w, &check_request("r1", HEAD_A, DIGEST), now).unwrap());
    let second = submit(&w, &check_request("r2", HEAD_A, DIGEST), now).unwrap();
    let joined = match second {
        Submission::Joined(record) => record,
        other => panic!("expected Submission::Joined, got {other:?}"),
    };
    assert_eq!(joined.id, first.id);
    assert_eq!(joined.requests, vec!["r1".to_string(), "r2".to_string()]);
    let current = load_current(&w, STAGE).unwrap().expect("a current record");
    assert_eq!(current.id, first.id);
    assert_eq!(current.state, JobState::Queued);
    let jobs: Vec<_> = fs::read_dir(stage_dir(&w, STAGE).unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_dir())
        .collect();
    assert_eq!(jobs.len(), 1, "{jobs:?}");
}

#[test]
fn a_new_head_supersedes_the_queued_job() {
    let (_dir, w) = state_root();
    let now = Utc::now();
    let old = queued(submit(&w, &check_request("r1", HEAD_A, DIGEST), now).unwrap());
    let (cancelled, new) = match submit(&w, &check_request("r2", HEAD_B, DIGEST), now).unwrap() {
        Submission::Superseded { cancelled, queued } => (cancelled, queued),
        other => panic!("expected Submission::Superseded, got {other:?}"),
    };
    assert_eq!(cancelled.id, old.id);
    assert_eq!(cancelled.state, JobState::Cancelled);
    let detail = cancelled.detail.clone().unwrap_or_default();
    assert!(detail.contains(&new.id), "{detail}");
    let current = load_current(&w, STAGE).unwrap().expect("a current record");
    assert_eq!(current.id, new.id);
    assert_eq!(current.state, JobState::Queued);
    assert_eq!(current.head, HEAD_B);
}

#[test]
#[serial]
fn cancel_kills_the_running_child() {
    let (_dir, w) = state_root();
    write_stage(&w, &["true"]);
    let mut runner = HostJobRunner::new(w.clone(), sleeper());
    queued(submit(&w, &check_request("r1", HEAD_A, DIGEST), Utc::now()).unwrap());
    let running = support::tick_until_running(&mut runner, &w);
    let pid = running.pid.unwrap();

    cancel(&w, STAGE, "takedown").unwrap();
    let ended = runner.tick().unwrap();
    assert!(
        wait_until(Duration::from_secs(5), || !alive(pid)),
        "child {pid} still exists after the cancel"
    );
    let current = load_current(&w, STAGE).unwrap().expect("a current record");
    assert_eq!(current.state, JobState::Cancelled);
    assert_eq!(current.detail.as_deref(), Some("takedown"));
    assert!(
        ended
            .iter()
            .any(|r| r.id == running.id && r.state == JobState::Cancelled),
        "the tick did not return the cancelled record: {ended:?}"
    );
}

/// Size of `path`, or 0 when it does not exist.
fn size(path: &std::path::Path) -> u64 {
    fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

#[test]
#[serial]
fn cancel_stops_the_criteria_of_a_running_job() {
    let name = "cancel_stops_the_criteria_of_a_running_job";
    if skip_unless(
        confined_commands_runnable(),
        name,
        "loom's confinement cannot start here",
    ) {
        return;
    }
    let fx = build_fixture(&["while :; do date >> beat.txt; sleep 0.2; done"]);
    let (head, digest) = (fx.head(), tree_digest(&fx.worktree).unwrap());
    let job = queued(
        submit(
            &fx.work_dir,
            &check_request("r1", &head, &digest),
            Utc::now(),
        )
        .unwrap(),
    );
    let (mut runner, _loom_home, _home) = verify_host_runner(fx.work_dir.clone());
    let beat = fx.worktree.join("beat.txt");
    let started = wait_until(Duration::from_secs(60), || {
        runner.tick().unwrap();
        std::thread::sleep(Duration::from_millis(100));
        beat.exists()
    });
    assert!(started, "the criterion never wrote beat.txt");
    assert!(!load_groups(&fx.work_dir, STAGE, &job.id)
        .unwrap()
        .is_empty());

    cancel(&fx.work_dir, STAGE, "takedown").unwrap();
    runner.tick().unwrap();
    let current = load_current(&fx.work_dir, STAGE)
        .unwrap()
        .expect("a current record");
    assert_eq!(current.state, JobState::Cancelled);
    let quiet = wait_until(Duration::from_secs(5), || {
        let before = size(&beat);
        std::thread::sleep(Duration::from_secs(1));
        before == size(&beat)
    });
    assert!(
        quiet,
        "the criterion kept writing beat.txt after the cancel"
    );
}

#[test]
#[serial]
fn a_job_past_its_deadline_is_failed_and_killed() {
    let (_dir, w) = state_root();
    write_stage(&w, &["true"]);
    let mut runner =
        HostJobRunner::new(w.clone(), sleeper()).with_deadline_override(Duration::from_secs(1));
    queued(submit(&w, &check_request("r1", HEAD_A, DIGEST), Utc::now()).unwrap());
    let running = support::tick_until_running(&mut runner, &w);
    let pid = running.pid.unwrap();
    std::thread::sleep(Duration::from_secs(2));
    let ended = runner.tick().unwrap();
    let failed = ended
        .iter()
        .find(|r| r.id == running.id)
        .unwrap_or_else(|| panic!("the tick did not return the job: {ended:?}"));
    assert_eq!(failed.state, JobState::Failed);
    let detail = failed.detail.clone().unwrap_or_default();
    assert!(detail.starts_with("deadline of"), "{detail}");
    let current = load_current(&w, STAGE).unwrap().expect("a current record");
    assert_eq!(current.state, JobState::Failed);
    assert!(
        wait_until(Duration::from_secs(5), || !alive(pid)),
        "child {pid} still exists past its deadline"
    );
}

#[test]
fn restart_interrupts_a_running_job() {
    let (_dir, w) = state_root();
    let mut exited = Command::new("true").spawn().unwrap();
    let pid = exited.id();
    exited.wait().unwrap();
    let mut running = record(STAGE, JobState::Running, HEAD_A, DIGEST);
    running.pid = Some(pid);
    save_current(&w, &running).unwrap();
    let waiting = record("stage-b", JobState::Queued, HEAD_A, DIGEST);
    save_current(&w, &waiting).unwrap();

    let changed = recover_after_restart(&w).unwrap();
    let current = load_current(&w, STAGE).unwrap().expect("stage-a record");
    assert_eq!(current.state, JobState::Interrupted);
    let detail = current.detail.clone().unwrap_or_default();
    assert!(detail.contains("restart"), "{detail}");
    assert!(changed.iter().any(|r| r.id == running.id), "{changed:?}");
    let other = load_current(&w, "stage-b")
        .unwrap()
        .expect("stage-b record");
    assert_eq!(other.state, JobState::Queued);
    assert_eq!(other.id, waiting.id);
}

#[test]
fn job_paths_refuse_traversal_and_symlinks() {
    let (_dir, w) = state_root();
    assert!(stage_dir(&w, "../x").is_err());
    assert!(job_dir(&w, STAGE, "../../x").is_err());
    assert!(job_dir(&w, STAGE, "ABC").is_err());

    let outside = TempDir::new().unwrap();
    std::os::unix::fs::symlink(outside.path(), w.join("verification")).unwrap();
    let job = record(STAGE, JobState::Queued, HEAD_A, DIGEST);
    assert!(
        save_current(&w, &job).is_err(),
        "followed a symlinked verification dir"
    );
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);

    let real = TempDir::new().unwrap();
    let links = TempDir::new().unwrap();
    let linked_root = links.path().join("work");
    std::os::unix::fs::symlink(real.path(), &linked_root).unwrap();
    save_current(&linked_root, &job).expect("a symlinked state root is accepted");
    assert_eq!(load_current(&linked_root, STAGE).unwrap(), Some(job));
}

/// Save a Running Check record for the fixture's clean worktree.
fn running_job(fx: &support::Fixture) -> JobRecord {
    let digest = tree_digest(&fx.worktree).unwrap();
    let job = record(STAGE, JobState::Running, &fx.head(), &digest);
    save_current(&fx.work_dir, &job).unwrap();
    job
}

#[test]
#[serial]
fn verify_host_records_each_criterion() {
    let name = "verify_host_records_each_criterion";
    if skip_unless(
        confined_commands_runnable(),
        name,
        "loom's confinement cannot start here",
    ) {
        return;
    }
    let fx = build_fixture(&["true", "false"]);
    let job = running_job(&fx);
    let elsewhere = TempDir::new().unwrap();
    let output = fx.verify_host(&job.id, elsewhere.path()).output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");

    let dir = job_dir(&fx.work_dir, STAGE, &job.id).unwrap();
    let result: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.join("result.json")).unwrap()).unwrap();
    assert_eq!(result["passed"], false, "{result}");
    let criteria = result["criteria"].as_array().expect("criteria array");
    let labels: Vec<&str> = criteria
        .iter()
        .map(|c| c["label"].as_str().unwrap())
        .collect();
    assert_eq!(labels, ["acceptance-0", "acceptance-1"]);
    let passed: Vec<bool> = criteria
        .iter()
        .map(|c| c["passed"].as_bool().unwrap())
        .collect();
    assert_eq!(passed, [true, false]);
    let progress = fs::read_to_string(dir.join("progress.jsonl")).unwrap();
    assert_eq!(progress.lines().count(), 2, "{progress}");
    let log = criteria[1]["log"]
        .as_str()
        .expect("acceptance-1 names a log");
    assert!(dir.join(log).is_file(), "missing log {log}");
}

#[test]
#[serial]
fn verify_host_refuses_inside_a_session() {
    let fx = build_fixture(&["true", "false"]);
    let job = running_job(&fx);
    let dir = job_dir(&fx.work_dir, STAGE, &job.id).unwrap();
    for (key, value) in [
        ("LOOM_SESSION_ID", support::SESSION),
        ("LOOM_STAGE_ID", STAGE),
    ] {
        let mut command = fx.verify_host(&job.id, &fx.worktree);
        let output = command.env(key, value).output().unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(2), "{key}: {stderr}");
        assert!(!dir.join("result.json").exists(), "{key}: wrote a result");
        let current = load_current(&fx.work_dir, STAGE).unwrap().expect("record");
        assert_eq!(current.state, JobState::Running, "{key}");
    }
}

#[test]
fn adjudication_sessions_may_relay_check() {
    for session in [
        SessionType::Adjudication,
        SessionType::Stage,
        SessionType::Knowledge,
    ] {
        let got = verdict(session, RequestKind::Check);
        assert_eq!(got, MatrixVerdict::Apply, "{session:?}");
    }
    for session in [SessionType::Contract, SessionType::Merge] {
        let got = verdict(session, RequestKind::Check);
        assert_eq!(got, MatrixVerdict::Refuse, "{session:?}");
    }
    assert!(!RequestKind::Check.is_control());
    let parsed: RequestKind = "check".parse().expect("check parses");
    assert_eq!(parsed.to_string(), "check");
}
