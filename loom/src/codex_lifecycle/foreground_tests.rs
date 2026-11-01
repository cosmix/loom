use std::fs;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde_json::json;

use super::forward_active_at;
use super::test_support::{Fixture, INV_A};

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
fn first_forward_is_bounded_and_bound_to_the_current_attempt() {
    let now = Utc::now();
    let (fixture, started) = authorized(now, 900);
    assert_eq!(
        observed(&fixture, started - chrono::Duration::seconds(1), now),
        Some(true)
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

#[test]
fn companion_needs_a_live_worker_and_fresh_bound_log() {
    let now = Utc::now();
    let (fixture, started) = authorized(now, 900);
    let auth = fixture.authorization("unit-1", INV_A).unwrap();
    let job_file = fixture.write_job(&auth, "task-one", "running").unwrap();
    let log = job_file.with_extension("log");
    fs::write(&log, "working").unwrap();
    let mut job: serde_json::Value = serde_json::from_slice(&fs::read(&job_file).unwrap()).unwrap();
    job["pid"] = json!(std::process::id());
    job["logFile"] = json!(&log);
    fs::write(&job_file, serde_json::to_vec(&job).unwrap()).unwrap();
    assert_eq!(
        observed(&fixture, started - chrono::Duration::seconds(1), now),
        Some(true)
    );

    let file = fs::File::open(&log).unwrap();
    file.set_modified((now - chrono::Duration::seconds(301)).into())
        .unwrap();
    assert_eq!(
        observed(&fixture, started - chrono::Duration::seconds(1), now),
        Some(false)
    );
}
