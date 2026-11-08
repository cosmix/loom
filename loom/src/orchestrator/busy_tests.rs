use super::*;
use chrono::Duration as ChronoDuration;
use tempfile::TempDir;

const ALL_OPS: [BusyOp; 3] = [
    BusyOp::BriefGeneration,
    BusyOp::MergeVerification,
    BusyOp::Provisioning,
];

fn busy_for(op: BusyOp, elapsed: i64, now: DateTime<Utc>) -> Busy {
    Busy {
        op,
        stage_id: "stage-a".to_string(),
        started_at: now - ChronoDuration::seconds(elapsed),
    }
}

/// A tick `age` seconds old, wrapping `op` for `elapsed` seconds when given.
fn tick_at(now: DateTime<Utc>, age: i64, busy: Option<(BusyOp, i64)>) -> Tick {
    Tick {
        recorded_at: now - ChronoDuration::seconds(age),
        phase: Some(Phase::Spawning),
        busy: busy.map(|(op, elapsed)| busy_for(op, elapsed, now)),
    }
}

#[test]
fn op_names_round_trip() {
    for op in ALL_OPS {
        assert_eq!(BusyOp::parse(op.as_str()), Some(op));
    }
    assert_eq!(BusyOp::parse("Provisioning"), None);
    assert_eq!(BusyOp::parse(""), None);
}

#[test]
fn ceilings_and_wording_match_the_contract() {
    let ceilings = ALL_OPS.map(BusyOp::ceiling_secs);
    assert_eq!(ceilings, [300, 1_200, 600]);
    assert_eq!(GLOBAL_BUSY_CEILING_SECS, 1_800);
    assert_eq!(
        BusyOp::BriefGeneration.describe("s"),
        "generating the brief for s"
    );
    assert_eq!(
        BusyOp::MergeVerification.describe("s"),
        "verifying the merge of s"
    );
    assert_eq!(BusyOp::Provisioning.describe("s"), "provisioning s");
}

#[test]
fn elapsed_saturates_at_zero() {
    let now = Utc::now();
    let future = Busy {
        started_at: now + ChronoDuration::minutes(5),
        ..busy_for(BusyOp::Provisioning, 0, now)
    };
    assert_eq!(future.elapsed_secs(now), 0);
    assert_eq!(
        busy_for(BusyOp::Provisioning, 42, now).elapsed_secs(now),
        42
    );
}

#[test]
fn no_tick_is_unknown() {
    assert_eq!(loop_state(None, Utc::now()), LoopState::Unknown);
}

#[test]
fn a_fresh_unwrapped_tick_is_ticking() {
    let now = Utc::now();
    let state = loop_state(Some(&tick_at(now, 59, None)), now);
    assert_eq!(state, LoopState::Ticking { age_secs: 59 });
}

#[test]
fn an_unwrapped_tick_stalls_at_sixty_seconds() {
    let now = Utc::now();
    let state = loop_state(Some(&tick_at(now, 60, None)), now);
    assert_eq!(
        state,
        LoopState::Stalled {
            age_secs: 60,
            busy: None,
            elapsed_secs: None
        }
    );
}

#[test]
fn a_busy_tick_flips_to_stalled_at_sixty_seconds_and_keeps_the_busy_line() {
    let now = Utc::now();
    let fresh = loop_state(
        Some(&tick_at(now, 59, Some((BusyOp::Provisioning, 100)))),
        now,
    );
    assert!(matches!(fresh, LoopState::Busy { .. }), "{fresh:?}");

    let stale = loop_state(
        Some(&tick_at(now, 60, Some((BusyOp::Provisioning, 100)))),
        now,
    );
    let LoopState::Stalled {
        age_secs,
        busy,
        elapsed_secs,
    } = stale
    else {
        panic!("a 60 s old tick is stalled, got {stale:?}");
    };
    assert_eq!(age_secs, 60);
    assert_eq!(busy.map(|b| b.op), Some(BusyOp::Provisioning));
    assert_eq!(elapsed_secs, Some(100));
}

#[test]
fn over_ceiling_flips_at_each_operations_ceiling() {
    let now = Utc::now();
    for op in ALL_OPS {
        let ceiling = op.ceiling_secs();
        for (elapsed, over) in [(ceiling - 1, false), (ceiling, true)] {
            let state = loop_state(Some(&tick_at(now, 1, Some((op, elapsed)))), now);
            let LoopState::Busy {
                elapsed_secs,
                over_ceiling,
                ..
            } = state
            else {
                panic!("{op:?} at {elapsed} s is busy, got {state:?}");
            };
            assert_eq!(elapsed_secs, elapsed);
            assert_eq!(over_ceiling, over, "{op:?} at {elapsed} s");
        }
    }
}

#[test]
fn the_global_ceiling_stalls_a_fresh_tick() {
    let now = Utc::now();
    let under = GLOBAL_BUSY_CEILING_SECS - 1;
    let state = loop_state(
        Some(&tick_at(now, 1, Some((BusyOp::Provisioning, under)))),
        now,
    );
    assert!(
        matches!(
            state,
            LoopState::Busy {
                over_ceiling: true,
                ..
            }
        ),
        "{state:?}"
    );

    let at = GLOBAL_BUSY_CEILING_SECS;
    let state = loop_state(
        Some(&tick_at(now, 1, Some((BusyOp::Provisioning, at)))),
        now,
    );
    let LoopState::Stalled {
        age_secs,
        busy,
        elapsed_secs,
    } = state
    else {
        panic!("the global ceiling is a stall, got {state:?}");
    };
    assert_eq!(age_secs, 1);
    assert!(busy.is_some());
    assert_eq!(elapsed_secs, Some(at));
}

#[test]
fn while_busy_returns_the_value_of_f() {
    let dir = TempDir::new().unwrap();
    let value = while_busy(
        dir.path(),
        Phase::Events,
        BusyOp::MergeVerification,
        "stage-a",
        || 7,
    );
    assert_eq!(value, 7);
}

#[test]
fn the_clearing_record_keeps_the_phase_it_was_given() {
    let dir = TempDir::new().unwrap();
    for phase in [Phase::Sync, Phase::Events] {
        while_busy(dir.path(), phase, BusyOp::BriefGeneration, "stage-a", || ());
        let after = tick::read(dir.path()).unwrap().unwrap();
        assert_eq!(after.phase, Some(phase));
        assert_eq!(after.busy, None);
    }
}

#[test]
fn tick_writes_leave_no_temp_file_behind() {
    let dir = TempDir::new().unwrap();
    let busy = busy_for(BusyOp::Provisioning, 0, Utc::now());

    tick::record(dir.path(), Phase::Idle);
    tick::record_busy(dir.path(), Phase::Spawning, &busy);
    while_busy(
        dir.path(),
        Phase::Spawning,
        BusyOp::Provisioning,
        "s",
        || (),
    );

    assert!(!dir.path().join("orchestrator.tick.tmp").exists());
    assert!(dir.path().join("orchestrator.tick").exists());
}
