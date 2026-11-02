use std::fs;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde_json::json;

use super::forward_active_at;
use super::test_support::{liquidmon, Fixture, INV_A, LIQUIDMON_RUNNING, LIQUIDMON_SUCCEEDED};
use super::CodexAuthorization;
use crate::orchestrator::monitor::direct_worker::live_tests::{
    mark_own_agent, nonce, spawn_companion_wait, spawn_tree, wait_for_supervisor, wait_for_worker,
    wait_until_gone,
};

const BUDGET: Duration = Duration::from_secs(300);

fn authorized(now: DateTime<Utc>, age_secs: i64) -> (Fixture, DateTime<Utc>) {
    let fixture = Fixture::new().unwrap();
    let started = now - chrono::Duration::seconds(age_secs);
    let mut row = fixture.authorization_value("unit-1", INV_A);
    row["ts"] = json!(started.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string());
    fixture.write_authorizations(&[row]).unwrap();
    (fixture, started)
}

fn observed(fixture: &Fixture, since: DateTime<Utc>, now: DateTime<Utc>) -> Option<bool> {
    forward_active_at(
        &fixture.work_dir,
        &fixture.stage,
        &fixture.session.id,
        since,
        BUDGET,
        now,
        &fixture.state_root,
    )
}

#[test]
fn authorization_without_a_live_worker_is_not_progress() {
    let now = Utc::now();
    let (fixture, started) = authorized(now, 900);
    assert_eq!(
        observed(&fixture, started - chrono::Duration::seconds(1), now),
        Some(false)
    );
    assert_eq!(observed(&fixture, now, now), None);
    assert_eq!(
        forward_active_at(
            &fixture.work_dir,
            &fixture.stage,
            "another-session",
            started,
            BUDGET,
            now,
            &fixture.state_root,
        ),
        None
    );
    assert_eq!(
        observed(
            &fixture,
            started - chrono::Duration::seconds(1),
            now + chrono::Duration::seconds(700)
        ),
        Some(false)
    );
}

/// A captured direct forward, with its row bound to this fixture: the capture
/// host's paths cannot pass canonical-path validation.
fn bound_direct(invocation: Option<&str>) -> (Fixture, CodexAuthorization) {
    let (fixture, auth) = liquidmon(&[LIQUIDMON_RUNNING]).unwrap();
    let bound = fixture.authorization_value("unit-1", INV_A);
    let mut row = fixture.liquidmon_authorization_value().unwrap();
    row["state_root"] = bound["state_root"].clone();
    row["companion_path"] = bound["companion_path"].clone();
    if let Some(invocation) = invocation {
        row["invocation_id"] = json!(invocation);
    }
    fixture.write_authorizations(&[row]).unwrap();
    (fixture, auth)
}

#[test]
fn direct_receipts_do_not_authorize_progress_or_shadow_later_work() {
    let (fixture, auth) = bound_direct(None);
    let now = auth.authorized_at + chrono::Duration::seconds(900);
    let since = auth.authorized_at - chrono::Duration::seconds(1);
    // A correlated START is lifecycle evidence, not host-owned launch or
    // progress evidence. It cannot hold recovery even within the wall cap.
    assert_eq!(observed(&fixture, since, now), Some(false));
    fixture
        .write_receipts(&[LIQUIDMON_RUNNING, LIQUIDMON_SUCCEEDED])
        .unwrap();
    assert_eq!(observed(&fixture, since, now), Some(false));
    // A later Bash call must still be eligible for generic foreground work
    // detection; this old forward contributes no current authorization.
    assert_eq!(observed(&fixture, now, now), None);
}

#[test]
fn only_the_authorized_launch_holds_recovery_for_a_running_direct_receipt() {
    let invocation = nonce(7);
    let (fixture, auth) = bound_direct(Some(&invocation));
    mark_own_agent(&fixture.work_dir, &fixture.session);
    // Past the captured receipt's observation, inside the first idle budget.
    let now = auth.authorized_at + chrono::Duration::seconds(295);
    let since = auth.authorized_at - chrono::Duration::seconds(1);
    assert_eq!(observed(&fixture, since, now), Some(false));

    let impostor = spawn_tree("--invocation-id inv-not-minted");
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(observed(&fixture, since, now), Some(false));
    drop(impostor);

    let worker = spawn_tree(&format!("--invocation-id {invocation}"));
    let supervisor = wait_for_supervisor(&invocation);
    wait_for_worker(supervisor);
    assert_eq!(observed(&fixture, since, now), Some(true));
    fixture
        .write_receipts(&[LIQUIDMON_RUNNING, LIQUIDMON_SUCCEEDED])
        .unwrap();
    assert_eq!(
        observed(&fixture, since, now),
        Some(false),
        "a finished forward holds nothing while any process lingers"
    );
    fixture.write_receipts(&[LIQUIDMON_RUNNING]).unwrap();
    assert_eq!(observed(&fixture, since, now), Some(true));
    drop(worker);
    wait_until_gone(&invocation);
    assert_eq!(observed(&fixture, since, now), Some(false));
}

#[test]
fn a_foreground_direct_forward_holds_before_its_first_receipt() {
    let now = Utc::now();
    let invocation = nonce(8);
    let fixture = Fixture::new().unwrap();
    let started = now - chrono::Duration::seconds(60);
    let mut row = fixture.authorization_value("unit-1", &invocation);
    row["ts"] = json!(started.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string());
    fixture.write_authorizations(&[row]).unwrap();
    mark_own_agent(&fixture.work_dir, &fixture.session);
    let since = started - chrono::Duration::seconds(1);
    assert_eq!(observed(&fixture, since, now), Some(false));
    let _worker = spawn_tree(&format!("--invocation-id {invocation}"));
    wait_for_worker(wait_for_supervisor(&invocation));
    assert_eq!(observed(&fixture, since, now), Some(true));
}

/// A running companion job on the stage-writable state root, with a fresh log.
fn plant_job(fixture: &Fixture, invocation: &str, pid: u32) -> std::path::PathBuf {
    let auth = fixture.authorization("unit-1", invocation).unwrap();
    let job_file = fixture.write_job(&auth, "task-one", "running").unwrap();
    let log = job_file.with_extension("log");
    fs::write(&log, "working").unwrap();
    let mut job: serde_json::Value = serde_json::from_slice(&fs::read(&job_file).unwrap()).unwrap();
    job["pid"] = json!(pid);
    job["logFile"] = json!(&log);
    fs::write(&job_file, serde_json::to_vec(&job).unwrap()).unwrap();
    log
}

fn authorized_invocation(
    now: DateTime<Utc>,
    age_secs: i64,
    tag: u64,
) -> (Fixture, String, DateTime<Utc>) {
    let invocation = nonce(tag);
    let fixture = Fixture::new().unwrap();
    let started = now - chrono::Duration::seconds(age_secs);
    let mut row = fixture.authorization_value("unit-1", &invocation);
    row["ts"] = json!(started.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string());
    fixture.write_authorizations(&[row]).unwrap();
    mark_own_agent(&fixture.work_dir, &fixture.session);
    (fixture, invocation, started)
}

#[test]
fn companion_needs_its_authorized_wait_and_a_fresh_bound_log() {
    let now = Utc::now();
    let (fixture, invocation, started) = authorized_invocation(now, 900, 9);
    let log = plant_job(&fixture, &invocation, std::process::id());
    let since = started - chrono::Duration::seconds(1);
    let _wait = spawn_companion_wait(&invocation, "task-one");
    assert_eq!(observed(&fixture, since, now), Some(true));

    let file = fs::File::open(&log).unwrap();
    file.set_modified((now - chrono::Duration::seconds(301)).into())
        .unwrap();
    assert_eq!(observed(&fixture, since, now), Some(false));
}

#[test]
fn a_planted_job_and_touched_log_hold_nothing_without_the_authorized_wait() {
    let now = Utc::now();
    let (fixture, invocation, started) = authorized_invocation(now, 60, 10);
    plant_job(&fixture, &invocation, 1);
    let since = started - chrono::Duration::seconds(1);
    assert_eq!(
        observed(&fixture, since, now),
        Some(false),
        "a job file alone proves no launch"
    );

    // Same names and shape under an id the guard never minted for this forward.
    let forged = spawn_companion_wait(&nonce(15), "task-one");
    assert_eq!(observed(&fixture, since, now), Some(false));
    drop(forged);

    // The authorized launch waiting on another job does not vouch for this one.
    let other = spawn_companion_wait(&invocation, "task-other");
    assert_eq!(observed(&fixture, since, now), Some(false));
    drop(other);

    // The real wait on a job whose worker is gone holds only as long as its log
    // stays fresh; pid 1 stands in for any live process a planted job can name.
    let _wait = spawn_companion_wait(&invocation, "task-one");
    assert_eq!(
        observed(&fixture, since, now),
        Some(true),
        "the known limit: inside the authorized launch the job file is still believed"
    );
}

#[test]
fn a_planted_job_cannot_shadow_or_extend_the_authorized_direct_worker() {
    let now = Utc::now();
    let (fixture, invocation, started) = authorized_invocation(now, 60, 11);
    plant_job(&fixture, &invocation, 1);
    let since = started - chrono::Duration::seconds(1);
    let worker = spawn_tree(&format!("--invocation-id {invocation}"));
    wait_for_worker(wait_for_supervisor(&invocation));
    assert_eq!(
        observed(&fixture, since, now),
        Some(true),
        "the direct worker is the evidence, whatever the job directory holds"
    );
    drop(worker);
    wait_until_gone(&invocation);
    assert_eq!(observed(&fixture, since, now), Some(false));
}

#[test]
fn a_parallel_tool_call_does_not_void_a_healthy_forward() {
    let now = Utc::now();
    let (fixture, invocation, _) = authorized_invocation(now, 60, 12);
    // The latest heartbeat progress is newer than the authorization.
    let later_progress = now - chrono::Duration::seconds(5);
    assert_eq!(
        observed(&fixture, later_progress, now),
        None,
        "with no live launch the old authorization is no evidence"
    );
    let worker = spawn_tree(&format!("--invocation-id {invocation}"));
    wait_for_worker(wait_for_supervisor(&invocation));
    assert_eq!(observed(&fixture, later_progress, now), Some(true));
    drop(worker);
    wait_until_gone(&invocation);
    assert_eq!(
        observed(&fixture, later_progress, now),
        None,
        "a forward that really ended stops holding"
    );
}

#[test]
fn a_parallel_tool_call_does_not_void_a_healthy_companion_forward() {
    let now = Utc::now();
    let (fixture, invocation, _) = authorized_invocation(now, 60, 13);
    plant_job(&fixture, &invocation, std::process::id());
    let later_progress = now - chrono::Duration::seconds(5);
    let wait = spawn_companion_wait(&invocation, "task-one");
    assert_eq!(observed(&fixture, later_progress, now), Some(true));
    drop(wait);
    assert_eq!(observed(&fixture, later_progress, now), None);
}

#[test]
fn an_old_forward_finished_by_its_receipt_holds_nothing_while_a_process_lingers() {
    let invocation = nonce(14);
    let (fixture, auth) = bound_direct(Some(&invocation));
    mark_own_agent(&fixture.work_dir, &fixture.session);
    let now = auth.authorized_at + chrono::Duration::seconds(295);
    let later_progress = auth.authorized_at + chrono::Duration::seconds(290);
    let _worker = spawn_tree(&format!("--invocation-id {invocation}"));
    wait_for_worker(wait_for_supervisor(&invocation));
    assert_eq!(observed(&fixture, later_progress, now), Some(true));
    fixture
        .write_receipts(&[LIQUIDMON_RUNNING, LIQUIDMON_SUCCEEDED])
        .unwrap();
    assert_eq!(observed(&fixture, later_progress, now), None);
}
