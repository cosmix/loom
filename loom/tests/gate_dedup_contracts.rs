//! Contracts of the gate-dedup stage: an accepted contract dispute re-freezes
//! the harness files with the contract, `loom check --criterion` runs only the
//! named criteria (locally and as a host job), and the plan lint and the
//! main-agent guard recognise an unfiltered vitest run.

#[path = "integration/helpers.rs"]
#[allow(dead_code)]
mod helpers;
#[path = "host_verification_contracts/support.rs"]
#[allow(dead_code)]
mod hv_support;
#[path = "gate_dedup_contracts/support.rs"]
mod support;

use std::fs;

use chrono::Utc;
use loom::host_verify::submit::{submit, Submission, SubmitRequest};
use loom::host_verify::JobKind;
use loom::relay::sha256_hex;
use serial_test::serial;
use tempfile::TempDir;

use support::{HELPER_FILE, STAGE};

#[test]
#[serial]
fn refreeze_covers_changed_harness_files() {
    let fx = support::refreeze_fixture();
    let new_bytes: &[u8] = b"pub fn ready() -> bool { true }\n";
    fs::write(fx.worktree.join(HELPER_FILE), new_bytes).unwrap();

    fx.apply().expect("the accept applies");

    assert_eq!(
        fx.frozen_hash(HELPER_FILE),
        Some(sha256_hex(new_bytes)),
        "the accept must re-freeze the harness file at its current bytes"
    );
    assert_eq!(fx.frozen_copy(HELPER_FILE), new_bytes);
}

#[test]
#[serial]
fn refreeze_drops_deleted_harness_files() {
    let fx = support::refreeze_fixture();
    fs::remove_file(fx.worktree.join(HELPER_FILE)).unwrap();

    fx.apply()
        .expect("an accept must not fail on a deleted harness file");

    assert_eq!(
        fx.frozen_hash(HELPER_FILE),
        None,
        "a deleted harness file must leave the freeze record"
    );
}

#[test]
#[serial]
fn check_criterion_runs_only_named_criteria() {
    let fx = hv_support::build_fixture(&["touch ran-0", "touch ran-1", "touch ran-2"]);
    let mut command = helpers::loom_cmd();
    command
        .args(["check", STAGE, "--criterion", "1"])
        .current_dir(&fx.worktree)
        .env("LOOM_WORK_DIR", &fx.work_dir)
        .env("HOME", &fx.home);
    for var in ["LOOM_SESSION_ID", "LOOM_STAGE_ID", "LOOM_SESSION_TYPE"] {
        command.env_remove(var);
    }
    hv_support::isolate_git(&mut command, &fx.home);
    let output = command.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(output.status.code(), Some(0), "{stdout}\n{stderr}");
    assert!(
        fx.worktree.join("ran-1").exists(),
        "criterion 1 did not run"
    );
    assert!(!fx.worktree.join("ran-0").exists(), "criterion 0 ran");
    assert!(!fx.worktree.join("ran-2").exists(), "criterion 2 ran");
    assert!(stdout.contains("touch ran-1"), "{stdout}");
}

fn check_request(request_id: &str, criteria: Option<Vec<usize>>) -> SubmitRequest<'_> {
    SubmitRequest {
        stage_id: STAGE,
        session_id: "session-a",
        request_id,
        kind: JobKind::Check,
        head: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        tree_digest: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        no_cache: false,
        criteria,
    }
}

#[test]
fn full_check_never_joins_a_subset_job() {
    let (_dir, work_dir) = hv_support::state_root();
    let now = Utc::now();
    let first = submit(&work_dir, &check_request("r1", Some(vec![1])), now).unwrap();
    assert!(matches!(first, Submission::Queued(_)), "{first:?}");

    let second = submit(&work_dir, &check_request("r2", None), now).unwrap();

    assert!(
        !matches!(second, Submission::Joined(_)),
        "a full check joined a one-criterion job: {second:?}"
    );
}

#[test]
fn plan_verify_flags_full_vitest_suite_in_standard_stage() {
    let temp = TempDir::new().unwrap();
    let verify = |name: &str, acceptance: &str| -> String {
        let plan = temp.path().join(name);
        support::write_v1_plan(&plan, acceptance);
        let output = helpers::loom_cmd()
            .current_dir(temp.path())
            .env("HOME", temp.path())
            .args(["plan", "verify"])
            .arg(&plan)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        format!("{stdout}\n{stderr}")
    };

    let full = verify("PLAN-full.md", "bunx vitest run");
    let flagged = full
        .lines()
        .any(|line| line.contains("runs the whole test suite") && line.contains("bunx vitest run"));
    assert!(
        flagged,
        "no full-suite warning naming the criterion:\n{full}"
    );

    let filtered = verify("PLAN-filtered.md", "bunx vitest run src/a.test.ts");
    assert!(
        !filtered.contains("runs the whole test suite"),
        "a filtered vitest run was flagged:\n{filtered}"
    );
}

#[test]
fn guard_blocks_main_agent_full_vitest_run() {
    let dir = support::temp_dir_no_claude();
    let hook = support::install_guard(dir.path());
    let work_dir = dir.path().join("work");
    support::write_standard_stage(&work_dir, STAGE);
    let work = work_dir.to_str().unwrap();
    let env = [("LOOM_WORK_DIR", work), ("LOOM_STAGE_ID", STAGE)];

    let (code, stderr) = support::run_guard_as_main(dir.path(), &hook, "bunx vitest run", &env);
    assert_eq!(
        code, 2,
        "the main agent's full vitest run must be blocked; stderr={stderr}"
    );
    assert!(
        stderr.contains("loom check"),
        "the refusal must name loom check: {stderr}"
    );

    let filtered = "bunx vitest run src/a.test.ts";
    let (code, stderr) = support::run_guard_as_main(dir.path(), &hook, filtered, &env);
    assert_eq!(
        code, 0,
        "a filtered vitest run must be allowed; stderr={stderr}"
    );
}
