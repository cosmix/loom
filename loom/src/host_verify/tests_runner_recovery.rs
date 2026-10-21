//! Runner recovery after a restart, and the environment the child runs in.

use chrono::Utc;
use serial_test::serial;
use tempfile::TempDir;

use super::runner::{job_deadline_secs, recover_after_restart, HostJobRunner};
use super::spawn::is_session_variable;
use super::store::{append_group, load_current, save_current};
use super::tests_runner::{
    queue, shell, state_root, tick_until, write_stage, Criterion, PASS, STAGE,
};
use super::{new_job_id, JobKind, JobRecord, JobState};
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

fn running_record(stage_id: &str, pid: Option<u32>) -> JobRecord {
    JobRecord {
        id: new_job_id(),
        stage_id: stage_id.to_string(),
        session_id: "session-a".to_string(),
        kind: JobKind::Check,
        head: "h".to_string(),
        tree_digest: "d".to_string(),
        no_cache: false,
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
    let mut criterion = Criterion::spawn();
    append_group(&work, STAGE, &interrupted.id, criterion.0.id()).unwrap();
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
    assert!(
        criterion.killed(),
        "a criterion group its leader still leads survived"
    );
    assert!(
        stranger.0.try_wait().unwrap().is_none(),
        "recovery signalled a pid whose command line names no job"
    );
}
