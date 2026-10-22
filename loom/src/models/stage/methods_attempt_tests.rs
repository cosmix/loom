//! Execution-time accounting: `Stage::begin_attempt` and `Stage::accumulate_attempt_time`.

use chrono::{Duration, Utc};

use crate::models::stage::{Stage, StageStatus};

#[test]
fn test_begin_attempt_initializes_execution_secs() {
    let mut stage = Stage::default();
    assert!(stage.execution_secs.is_none());
    assert!(stage.attempt_started_at.is_none());

    let now = Utc::now();
    stage.begin_attempt(now);

    assert_eq!(stage.execution_secs, Some(0));
    assert_eq!(stage.attempt_started_at, Some(now));
}

#[test]
fn test_begin_attempt_preserves_existing_execution_secs() {
    let mut stage = Stage {
        execution_secs: Some(60),
        ..Stage::default()
    };

    let now = Utc::now();
    stage.begin_attempt(now);

    assert_eq!(stage.execution_secs, Some(60)); // Preserved
    assert_eq!(stage.attempt_started_at, Some(now));
}

#[test]
fn test_accumulate_attempt_time() {
    let mut stage = Stage::default();
    let start = Utc::now() - Duration::seconds(30);
    stage.attempt_started_at = Some(start);
    stage.execution_secs = Some(0);

    stage.accumulate_attempt_time(Utc::now());

    assert!(stage.execution_secs.unwrap() >= 29); // Allow for timing variance
    assert!(stage.attempt_started_at.is_none()); // Cleared
}

#[test]
fn test_accumulate_attempt_time_adds_to_existing() {
    let start = Utc::now() - Duration::seconds(50);
    let mut stage = Stage {
        execution_secs: Some(100),
        attempt_started_at: Some(start),
        ..Stage::default()
    };

    stage.accumulate_attempt_time(Utc::now());

    assert!(stage.execution_secs.unwrap() >= 149); // 100 + ~50
    assert!(stage.attempt_started_at.is_none());
}

#[test]
fn test_accumulate_attempt_time_noop_without_started() {
    let mut stage = Stage {
        execution_secs: Some(100),
        ..Stage::default()
    };
    // attempt_started_at is None

    stage.accumulate_attempt_time(Utc::now());

    assert_eq!(stage.execution_secs, Some(100)); // Unchanged
}

#[test]
fn test_try_mark_blocked_banks_running_attempt() {
    let mut stage = Stage {
        status: StageStatus::Executing,
        execution_secs: Some(10),
        attempt_started_at: Some(Utc::now() - Duration::seconds(3600)),
        ..Stage::default()
    };

    stage.try_mark_blocked().unwrap();

    assert!(stage.execution_secs.unwrap() >= 3609);
    assert!(stage.attempt_started_at.is_none());
}

#[test]
fn test_try_mark_blocked_after_banking_adds_nothing() {
    let mut stage = Stage {
        status: StageStatus::Executing,
        execution_secs: Some(120),
        attempt_started_at: None,
        ..Stage::default()
    };

    stage.try_mark_blocked().unwrap();

    assert_eq!(stage.execution_secs, Some(120));
}
