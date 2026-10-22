//! End-to-end host verification: an in-session `loom stage complete`, the
//! real `loom-relay.sh` hook, `Orchestrator::drain_session_inboxes`, then the
//! production `Orchestrator::tick_host_jobs` running the built loom binary's
//! `stage verify-host` and applying its result, and the session's next
//! `loom stage complete` reading the outcome. A freeze job whose contract
//! never ends is stopped by the job deadline. No socket is opened.

#[path = "integration/helpers.rs"]
#[allow(dead_code)]
mod helpers;
#[path = "daemon_completion_contracts/support.rs"]
#[allow(dead_code)]
mod support;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use chrono::Utc;
use loom::handoff::generator::load_trusted_session_checkpoint;
use loom::handoff::schema::CompletionPhase;
use loom::host_verify::runner::{HostJobRunner, RunnerConfig};
use loom::host_verify::store::{load_current, load_groups, load_job_file, load_job_record};
use loom::host_verify::submit::{submit, Submission, SubmitRequest};
use loom::host_verify::{digest, JobKind, JobRecord, JobState};
use loom::models::stage::StageStatus;
use loom::orchestrator::{Orchestrator, OrchestratorConfig};
use loom::plan::schema::ContractSpec;
use loom::process::sandbox_probe::{confined_commands_runnable, skip_unless};
use loom::verify::transitions::{load_stage, update_stage};
use serial_test::serial;
use tempfile::TempDir;

use support::{build_fixture, isolate_git, Fixture, SESSION, STAGE};

/// The longest a job may take to run and be applied.
const JOB_BUDGET: Duration = Duration::from_secs(120);

/// True (after reporting the skip) when loom's confinement cannot start
/// here, or a tool the relay hook needs is missing.
fn skip(test_name: &str) -> bool {
    if skip_unless(
        confined_commands_runnable(),
        test_name,
        "loom's confinement cannot start here",
    ) {
        return true;
    }
    let present = ["jq", "bash", "git"]
        .iter()
        .all(|tool| helpers::tool_available(tool));
    skip_unless(present, test_name, "jq, bash or git was not found on PATH")
}

/// The environment the daemon's child inherits: a scratch user directory
/// with the update check off, and the terminal `Orchestrator::new` detects.
struct DaemonEnv {
    _home: TempDir,
    _guards: [helpers::EnvVarGuard; 2],
}

fn daemon_env() -> DaemonEnv {
    let home = TempDir::new().expect("create scratch LOOM_HOME");
    std::fs::write(home.path().join("config.toml"), "[update]\ncheck = false\n")
        .expect("write scratch user config");
    let guards = [
        helpers::EnvVarGuard::set("LOOM_HOME", home.path()),
        helpers::EnvVarGuard::set("LOOM_TERMINAL", "xterm"),
    ];
    DaemonEnv {
        _home: home,
        _guards: guards,
    }
}

/// `loom stage complete stage-a` as the stage session.
fn complete_in_session(fx: &Fixture) -> Output {
    fx.session_loom()
        .args(["stage", "complete", STAGE])
        .stdin(Stdio::null())
        .output()
        .expect("spawn loom stage complete")
}

fn first_line(output: &Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.lines().next().unwrap_or_default().to_string()
}

/// Run the real `loom-relay.sh` on the Bash call that printed `output`, as a
/// direct child of this process, so its `loom hook relay` grandchild sits
/// inside the recorded session process tree.
fn run_hook(fx: &Fixture, output: &Output) -> Output {
    let payload = serde_json::json!({
        "tool_name": "Bash",
        "tool_input": {"command": format!("loom stage complete {STAGE}")},
        "tool_response": {
            "stdout": String::from_utf8_lossy(&output.stdout),
            "stderr": String::from_utf8_lossy(&output.stderr),
        },
    })
    .to_string();
    let hook_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../loom-hooks/loom-relay.sh");
    let real_path = std::env::var_os("PATH").unwrap_or_default();
    let mut command = Command::new("bash");
    helpers::clear_relay_env(&mut command);
    command
        .arg(&hook_path)
        .env("LOOM_SESSION_ID", SESSION)
        .env("LOOM_STAGE_ID", STAGE)
        .env("LOOM_SCRATCH_DIR", &fx.scratch_dir)
        .env("LOOM_WORK_DIR", &fx.work_dir)
        .env("XDG_RUNTIME_DIR", &fx.xdg)
        .env("HOME", &fx.home)
        .env("LOOM_BIN", helpers::loom_bin_path())
        .env("LOOM_HOOK_PATH", &real_path)
        .env("PATH", &real_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    isolate_git(&mut command, &fx.home);
    let mut child = command.spawn().expect("spawn loom-relay.sh");
    let stdin = child.stdin.take().expect("child stdin is piped");
    { stdin }
        .write_all(payload.as_bytes())
        .expect("write hook payload");
    child.wait_with_output().expect("wait for loom-relay.sh")
}

/// One in-process orchestrator over the fixture's state root, kept for the
/// whole test, whose runner starts the built loom binary.
fn orchestrator(fx: &Fixture, deadline: Option<Duration>) -> Orchestrator {
    let repo_root: PathBuf = fx.work_dir.parent().and_then(Path::parent).unwrap().into();
    let config = OrchestratorConfig {
        work_dir: fx.work_dir.clone(),
        repo_root,
        manual_mode: true,
        enable_skill_routing: false,
        ..Default::default()
    };
    let graph = helpers::build_test_graph(vec![(STAGE, vec![])]);
    let mut orchestrator = Orchestrator::new(config, graph).expect("construct orchestrator");
    let runner_config = RunnerConfig {
        program: PathBuf::from(helpers::loom_bin_path()),
        args_prefix: vec!["stage".to_string(), "verify-host".to_string()],
        slots: 1,
    };
    let runner = HostJobRunner::new(fx.work_dir.clone(), runner_config);
    let runner = match deadline {
        Some(deadline) => runner.with_deadline_override(deadline),
        None => runner,
    };
    orchestrator.set_host_job_runner(runner);
    orchestrator
}

/// Tick the production host-job pass every 200 ms until `job` carries its
/// `applied` marker; returns the job's final record.
fn tick_until_applied(fx: &Fixture, orchestrator: &mut Orchestrator, job: &str) -> JobRecord {
    let start = Instant::now();
    while start.elapsed() < JOB_BUDGET {
        orchestrator.tick_host_jobs();
        let marker = load_job_file(&fx.work_dir, STAGE, job, "applied").expect("read marker");
        if marker.is_some() {
            let record = load_job_record(&fx.work_dir, STAGE, job)
                .expect("read job record")
                .expect("the job keeps its record");
            println!("e2e: ran {:?}", record.state);
            println!("e2e: applied");
            return record;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    panic!("host job {job} was not applied within {JOB_BUDGET:?}");
}

/// Commit a change, request the completion in the session, relay it, drain
/// once and run the job to its application. Returns the orchestrator.
fn relay_and_run_completion(fx: &Fixture) -> Orchestrator {
    fx.commit_change("a.txt");
    let requested = complete_in_session(fx);
    let stdout = String::from_utf8_lossy(&requested.stdout).into_owned();
    assert_eq!(requested.status.code(), Some(0), "{stdout}");
    let first = first_line(&requested);
    assert!(
        first.starts_with("Host job: requested (complete, "),
        "{stdout}"
    );
    let last = stdout.lines().next_back().unwrap_or_default();
    assert!(last.starts_with("LOOM_RELAY_V1"), "{stdout}");

    let hook = run_hook(fx, &requested);
    let hook_stderr = String::from_utf8_lossy(&hook.stderr);
    assert!(
        hook.status.success(),
        "loom-relay.sh must exit 0: {hook_stderr}"
    );

    let mut orchestrator = orchestrator(fx, None);
    orchestrator.drain_session_inboxes();
    let job = load_current(&fx.work_dir, STAGE)
        .expect("read the current job")
        .expect("the drain queued a host job");
    assert_eq!(job.kind, JobKind::Complete);
    println!("e2e: queued {}", job.id);
    tick_until_applied(fx, &mut orchestrator, &job.id);
    orchestrator
}

#[test]
#[serial]
fn a_passing_host_completion_completes_the_stage() {
    if skip("a_passing_host_completion_completes_the_stage") {
        return;
    }
    let _env = daemon_env();
    let fx = build_fixture(&["true"]);
    let _orchestrator = relay_and_run_completion(&fx);

    let stage = load_stage(STAGE, &fx.work_dir).expect("load stage");
    assert_eq!(stage.status, StageStatus::Completed);
    let outcome = complete_in_session(&fx);
    let first = first_line(&outcome);
    assert!(first.starts_with("Host job: completed"), "{first}");
    assert_eq!(outcome.status.code(), Some(0));
}

#[test]
#[serial]
fn a_failing_host_completion_leaves_the_stage_executing() {
    if skip("a_failing_host_completion_leaves_the_stage_executing") {
        return;
    }
    let _env = daemon_env();
    let fx = build_fixture(&["false"]);
    let _orchestrator = relay_and_run_completion(&fx);

    let stage = load_stage(STAGE, &fx.work_dir).expect("load stage");
    assert_eq!(stage.status, StageStatus::Executing);
    let attempt = load_trusted_session_checkpoint(STAGE, SESSION, &fx.work_dir)
        .expect("read the checkpoint")
        .expect("a trusted completion checkpoint")
        .latest
        .expect("a latest completion attempt");
    assert_eq!(attempt.phase, CompletionPhase::ToolFailed);
    let outcome = complete_in_session(&fx);
    let first = first_line(&outcome);
    assert!(first.starts_with("Host job: failed"), "{first}");
    assert_eq!(outcome.status.code(), Some(1));
}

/// Make stage-a a v2 contract stage whose one contract never ends.
fn never_ending_contract_stage(fx: &Fixture) {
    let file = "tests/never_contract.rs";
    std::fs::create_dir_all(fx.worktree.join("tests")).expect("create tests dir");
    std::fs::write(fx.worktree.join(file), "#[test]\nfn never() {}\n").expect("write contract");
    update_stage(STAGE, &fx.work_dir, |stage| {
        stage.plan_version = 2;
        stage.contracts = vec![ContractSpec {
            id: "never-ends".to_string(),
            file: file.to_string(),
            test: "sleep 600".to_string(),
            runner: None,
            cwd: None,
            scenario: "the contract runs".to_string(),
            rejects: "nothing: it never ends".to_string(),
        }];
        Ok(())
    })
    .expect("make stage-a a contract stage");
}

#[test]
#[serial]
fn a_contract_that_never_ends_is_stopped_by_the_job_deadline() {
    if skip("a_contract_that_never_ends_is_stopped_by_the_job_deadline") {
        return;
    }
    let _env = daemon_env();
    let fx = build_fixture(&["true"]);
    never_ending_contract_stage(&fx);
    let (head, tree) = (fx.head(), digest::tree_digest(&fx.worktree).unwrap());
    let request = SubmitRequest {
        stage_id: STAGE,
        session_id: SESSION,
        request_id: "r1",
        kind: JobKind::Freeze,
        head: &head,
        tree_digest: &tree,
        no_cache: false,
        criteria: None,
    };
    let job = match submit(&fx.work_dir, &request, Utc::now()).expect("submit") {
        Submission::Queued(job) => job,
        other => panic!("expected a queued freeze job, got {other:?}"),
    };
    let mut orchestrator = orchestrator(&fx, Some(Duration::from_secs(5)));

    let record = tick_until_applied(&fx, &mut orchestrator, &job.id);

    assert_eq!(record.state, JobState::Failed, "{record:?}");
    let detail = record.detail.clone().unwrap_or_default();
    assert!(detail.contains("deadline of"), "{detail}");
    for pgid in load_groups(&fx.work_dir, STAGE, &job.id).expect("read groups") {
        let group = nix::unistd::Pid::from_raw(i32::try_from(pgid).unwrap());
        let alive = nix::sys::signal::killpg(group, None);
        assert_eq!(alive, Err(nix::errno::Errno::ESRCH), "group {pgid} lives");
    }
    let stage = load_stage(STAGE, &fx.work_dir).expect("load stage");
    assert_eq!(stage.status, StageStatus::WaitingForInput);
    let reason = stage.review_reason.unwrap_or_default();
    assert!(reason.contains("deadline of"), "{reason}");
}
