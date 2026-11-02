//! A takedown over a record with no PID identity.

use super::*;

/// A takedown cannot confirm the death of a record with no PID identity while
/// its launch may still start it; once too old for that, the record must not
/// keep the stage in handoff for good.
#[test]
fn a_takedown_ends_an_old_record_with_no_identity_and_retains_a_fresh_one() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    executing_stage(&work);
    let mut orchestrator = orchestrator_for(&work, temp.path());
    let reason = crate::models::session::SessionExitReason::Replaced;

    let fresh = recorded_session(&work);
    let survivors = orchestrator
        .take_down_agents("test-stage", vec![fresh.clone()], reason)
        .unwrap();
    assert_eq!(survivors, vec![fresh.id.clone()]);

    let mut old = fresh;
    old.created_at = chrono::Utc::now() - chrono::Duration::seconds(300);
    save_session(&old, &work).unwrap();
    let survivors = orchestrator
        .take_down_agents("test-stage", vec![old.clone()], reason)
        .unwrap();
    assert!(survivors.is_empty(), "{survivors:?}");
    let stored = load_session_exact(&work, &old.id).unwrap().unwrap();
    assert_eq!(stored.status, SessionStatus::ContextExhausted);
}
