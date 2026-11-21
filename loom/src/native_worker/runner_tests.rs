//! Runner tests. The supervisor stand-in is a shell script passed through
//! `with_program`; it appends its arguments to `calls.log`. Every test waits
//! for the processes it started, so none is left running.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};

use super::*;
use crate::native_worker::ids::mint_invocation_id;
use crate::native_worker::index::unit_state;
use crate::native_worker::model::{NativeManifest, NativeUnitState, Ownership, Phase};
use crate::native_worker::store::{create_invocation, read_cancel};

const STAGE: &str = "runner-unit";

/// Writes `supervisor.json` like the real supervisor, then waits up to 10 s
/// for `control/cancel.json`.
const CLAIM_AND_WAIT: &str = r#"start=$(cut -d' ' -f22 /proc/$$/stat)
printf '{"pid":%s,"start":"%s"}' "$$" "$start" > "$4/supervisor.json"
i=0
while [ ! -e "$4/control/cancel.json" ] && [ "$i" -lt 100 ]; do sleep 0.1; i=$((i+1)); done
"#;

struct Fx {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    work: PathBuf,
}

fn fx() -> Fx {
    let tmp = tempfile::TempDir::new().expect("temp dir");
    let root = tmp.path().canonicalize().expect("canonical");
    let work = root.join("repo").join(".loom").join("work");
    fs::create_dir_all(&work).expect("work dir");
    Fx {
        _tmp: tmp,
        root,
        work,
    }
}

impl Fx {
    /// A 0755 stand-in that logs `$*` and then runs `body`.
    fn program(&self, body: &str) -> PathBuf {
        let path = self.root.join("supervise.sh");
        let calls = self.calls_path();
        let script = format!("#!/bin/sh\necho \"$*\" >> '{}'\n{body}", calls.display());
        fs::write(&path, script).expect("script");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod");
        path
    }

    fn calls_path(&self) -> PathBuf {
        self.root.join("calls.log")
    }

    fn calls(&self) -> Vec<String> {
        fs::read_to_string(self.calls_path())
            .map(|text| text.lines().map(str::to_string).collect())
            .unwrap_or_default()
    }

    fn record(&self, created_at: DateTime<Utc>) -> (String, PathBuf) {
        let invocation_id = mint_invocation_id().expect("id");
        let manifest = NativeManifest {
            v: 1,
            stage_id: STAGE.to_string(),
            unit_id: "unit-a".to_string(),
            invocation_id: invocation_id.clone(),
            owner_session_id: "session-lead".to_string(),
            workspace_root: self.root.clone(),
            brief_path: self.root.join("brief.md"),
            brief_sha256: crate::utils::sha256_hex(b"brief"),
            model: "gpt-6.1-terra".to_string(),
            effort: "xhigh".to_string(),
            deadline_secs: 1500,
            created_at,
        };
        let dir = create_invocation(&self.work, &manifest, b"brief").expect("record");
        (invocation_id, dir)
    }

    fn state(&self) -> NativeUnitState {
        unit_state(&self.work, STAGE, "unit-a")
            .expect("state")
            .expect("unit ran")
    }

    fn runner(&self, program: PathBuf) -> NativeRunner {
        NativeRunner::new(self.work.clone()).with_program(program)
    }
}

fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Ticks until the record is terminal with ownership settled.
fn tick_until_settled(fx: &Fx, runner: &mut NativeRunner) -> NativeUnitState {
    wait_until("the record to settle", || {
        runner.tick();
        let state = fx.state();
        state.phase == Phase::Terminal && state.ownership != Ownership::Held
    });
    assert!(runner.held.is_empty(), "a child is still held");
    fx.state()
}

#[test]
#[serial_test::serial]
fn start_happens_once_per_invocation() {
    let fx = fx();
    let (_, dir) = fx.record(Utc::now());
    let mut runner = fx.runner(fx.program("exit 0\n"));
    for _ in 0..3 {
        runner.tick();
        std::thread::sleep(Duration::from_millis(100));
    }
    tick_until_settled(&fx, &mut runner);
    runner.tick();
    let expected = format!("subagents supervise --dir {}", dir.display());
    assert_eq!(fx.calls(), vec![expected]);
}

#[test]
#[serial_test::serial]
fn a_new_runner_does_not_start_a_claimed_invocation() {
    let fx = fx();
    let (_, dir) = fx.record(Utc::now());
    let program = fx.program(CLAIM_AND_WAIT);
    let mut first = fx.runner(program.clone());
    first.tick();
    wait_until("the claim", || dir.join("supervisor.json").exists());
    let mut second = fx.runner(program);
    for _ in 0..3 {
        second.tick();
    }
    assert!(second.held.is_empty(), "the second runner spawned");
    assert_eq!(fx.calls().len(), 1);
    first.cancel_all();
    assert!(first.held.is_empty(), "the stand-in outlived cancel_all");
}

#[test]
#[serial_test::serial]
fn a_spawn_failure_ends_the_record_failed_and_released() {
    let fx = fx();
    fx.record(Utc::now());
    let mut runner = fx.runner(fx.root.join("no-such-program"));
    runner.tick();
    let state = fx.state();
    assert_eq!(state.outcome, Some(Outcome::Failed));
    assert_eq!(state.ownership, Ownership::Released);
    let reason = state.outcome_reason.unwrap_or_default();
    assert!(
        reason.starts_with("cannot start the supervisor: "),
        "{reason}"
    );
    runner.tick();
    assert!(runner.held.is_empty());
}

#[test]
#[serial_test::serial]
fn an_exited_supervisor_without_terminal_is_settled_unknown_released() {
    let fx = fx();
    let (_, dir) = fx.record(Utc::now());
    let mut runner = fx.runner(fx.program("exit 3\n"));
    let state = tick_until_settled(&fx, &mut runner);
    assert_eq!(state.outcome, Some(Outcome::Unknown));
    assert_eq!(
        state.outcome_reason.as_deref(),
        Some("supervisor exited without a terminal record")
    );
    assert_eq!(state.ownership, Ownership::Released);
    let claim = fs::read_to_string(dir.join("supervisor.json")).expect("tombstone");
    assert!(claim.contains("\"pid\":0"), "{claim}");
}

// The child reads its session id from /proc, which macOS lacks.
#[cfg(target_os = "linux")]
#[test]
#[serial_test::serial]
fn a_child_runs_in_its_own_session() {
    let fx = fx();
    let out = fx.root.join("session.txt");
    let body = format!(
        "echo \"$$ $(cut -d' ' -f6 /proc/$$/stat)\" > '{}'\n",
        out.display()
    );
    fx.record(Utc::now());
    let mut runner = fx.runner(fx.program(&body));
    tick_until_settled(&fx, &mut runner);
    let text = fs::read_to_string(&out).expect("session.txt");
    let fields: Vec<&str> = text.split_whitespace().collect();
    assert_eq!(fields.len(), 2, "{text}");
    assert_eq!(fields[0], fields[1], "the child is not a session leader");
    assert_ne!(fields[0], std::process::id().to_string());
}

#[test]
#[serial_test::serial]
fn session_variables_are_stripped() {
    let fx = fx();
    let out = fx.root.join("env.txt");
    let body = format!("env > '{}'\n", out.display());
    fx.record(Utc::now());
    std::env::set_var("LOOM_SESSION_ID", "session-x");
    std::env::set_var("LOOM_STAGE_ID", "stage-x");
    let previous_home = std::env::var_os("LOOM_HOME");
    std::env::set_var("LOOM_HOME", fx.root.join("home"));
    let mut runner = fx.runner(fx.program(&body));
    tick_until_settled(&fx, &mut runner);
    std::env::remove_var("LOOM_SESSION_ID");
    std::env::remove_var("LOOM_STAGE_ID");
    match previous_home {
        Some(home) => std::env::set_var("LOOM_HOME", home),
        None => std::env::remove_var("LOOM_HOME"),
    }
    let env = fs::read_to_string(&out).expect("env.txt");
    let lines: Vec<&str> = env.lines().collect();
    assert!(!lines
        .iter()
        .any(|line| line.starts_with("LOOM_SESSION_ID=")));
    assert!(!lines.iter().any(|line| line.starts_with("LOOM_STAGE_ID=")));
    assert!(lines.iter().any(|line| line.starts_with("LOOM_HOME=")));
    let work = format!("LOOM_WORK_DIR={}", fx.work.display());
    assert!(lines.contains(&work.as_str()), "{env}");
}

#[test]
#[serial_test::serial]
fn cancel_all_writes_cancel_json_and_waits() {
    let fx = fx();
    let (_, dir) = fx.record(Utc::now());
    let mut runner = fx.runner(fx.program(CLAIM_AND_WAIT));
    runner.tick();
    wait_until("the claim", || dir.join("supervisor.json").exists());
    let started = Instant::now();
    runner.cancel_all();
    assert!(
        started.elapsed() < STOP_WAIT,
        "cancel_all waited the full bound"
    );
    assert!(runner.held.is_empty(), "the stand-in is still held");
    let cancel = read_cancel(&dir).expect("cancel").expect("cancel.json");
    assert_eq!(
        (cancel.by.as_str(), cancel.reason.as_str()),
        ("daemon", "daemon stopped")
    );
    let state = fx.state();
    assert_eq!(state.outcome, Some(Outcome::Unknown));
    assert_eq!(state.ownership, Ownership::Released);
}

#[test]
#[serial_test::serial]
fn recovery_settles_old_unclaimed_records_and_leaves_young_ones() {
    let fx = fx();
    let (young, _) = fx.record(Utc::now());
    let old_at = Utc::now() - chrono::Duration::seconds(60);
    let (old, old_dir) = fx.record(old_at);
    assert_eq!(recover_after_restart(&fx.work), vec![old.clone()]);
    assert!(old_dir.join("supervisor.json").exists(), "no tombstone");
    let young_dir = store::invocation_dir(&fx.work, STAGE, &young);
    assert!(!young_dir.join("supervisor.json").exists());
    assert!(
        recover_after_restart(&fx.work).is_empty(),
        "settling is not idempotent"
    );
    assert!(fx.calls().is_empty(), "recovery started a supervisor");
}

#[test]
#[serial_test::serial]
fn a_live_claim_from_an_earlier_daemon_is_adopted() {
    let fx = fx();
    let (_, dir) = fx.record(Utc::now() - chrono::Duration::seconds(60));
    let own = std::process::id();
    let start = crate::process::process_start_time(own).expect("start time");
    let claim = format!("{{\"pid\":{own},\"start\":\"{start}\"}}");
    fs::write(dir.join("supervisor.json"), claim).expect("claim");
    let mut runner = fx.runner(fx.program("exit 0\n"));
    runner.tick();
    assert!(recover_after_restart(&fx.work).is_empty());
    assert_eq!(
        fx.state().phase,
        Phase::Admitted,
        "a live supervisor was settled"
    );
    assert!(fx.calls().is_empty(), "a claimed invocation was started");
}

#[test]
fn a_lost_supervisor_with_live_namespace_members_is_left_unverified() {
    let fx = fx();
    let (_, dir) = fx.record(Utc::now());
    let identity = crate::native_worker::model::BoundaryIdentity {
        supervisor_pid: i32::MAX - 7,
        supervisor_start: "0".to_string(),
        boundary_pid: i32::MAX - 7,
        pid_namespace: crate::native_worker::ingress_contract_tests::own_pid_namespace(),
        boot_id: crate::process::boot_id::os_boot_id().expect("boot id"),
        mac: None,
    };
    store::write_boundary(&dir, &identity).expect("boundary.json");
    settle_lost_supervisor(&dir).expect("settle");
    settle_lost_supervisor(&dir).expect("settle again");
    let state = fx.state();
    assert_eq!(state.outcome, Some(Outcome::Unknown));
    assert_eq!(state.ownership, Ownership::Unverified);
    let events = store::read_events(&dir).expect("events");
    assert_eq!(events.len(), 3, "settling twice appended twice: {events:?}");
}
