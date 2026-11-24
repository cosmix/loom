//! Contracts for the idle-stop decision behind `loom hook idle-stop`: an unattended stop of an
//! executing stage session with nothing pending is blocked, at most `MAX_IDLE_STOP_BLOCKS` times
//! and once per tool call, and every fact that shows a wait, or cannot be read, lets it stop.

use super::idle_stop::{decide, StopFacts, MAX_IDLE_STOP_BLOCKS};

/// The one combination that blocks: the session owns its executing stage, is a stage session,
/// has no active child, no live host job, and has run a tool since its last block.
fn blocking() -> StopFacts {
    StopFacts {
        owns_executing_stage: true,
        stage_session: true,
        children_quiet: Some(true),
        host_job_live: false,
        worked_since_last_block: true,
        blocks_so_far: 0,
    }
}

#[test]
fn an_unattended_stop_is_blocked_once_per_tool_call() {
    let reason = decide("s1", &blocking())
        .unwrap_or_else(|| panic!("an idle executing session was let stop: {:?}", blocking()));
    for needle in ["s1", "loom stage block", "loom stage complete"] {
        assert!(
            reason.contains(needle),
            "the block reason lacks {needle:?}: {reason}"
        );
    }

    let no_tool_since = StopFacts {
        worked_since_last_block: false,
        ..blocking()
    };
    assert_eq!(
        decide("s1", &no_tool_since),
        None,
        "a stop with no tool call since the last block was blocked again"
    );

    let last_allowed = StopFacts {
        blocks_so_far: MAX_IDLE_STOP_BLOCKS - 1,
        ..blocking()
    };
    assert!(
        decide("s1", &last_allowed).is_some(),
        "a stop below the block limit was let through: {last_allowed:?}"
    );

    let spent = StopFacts {
        blocks_so_far: MAX_IDLE_STOP_BLOCKS,
        ..blocking()
    };
    assert_eq!(
        decide("s1", &spent),
        None,
        "a stop at the block limit was blocked"
    );
}

#[test]
fn a_wait_or_an_unreadable_fact_is_never_blocked() {
    let cases = [
        (
            "a child is active",
            StopFacts {
                children_quiet: Some(false),
                ..blocking()
            },
        ),
        (
            "the child evidence is unreadable",
            StopFacts {
                children_quiet: None,
                ..blocking()
            },
        ),
        (
            "a host job is live",
            StopFacts {
                host_job_live: true,
                ..blocking()
            },
        ),
        (
            "the session is no stage session",
            StopFacts {
                stage_session: false,
                ..blocking()
            },
        ),
        (
            "the session does not own an executing stage",
            StopFacts {
                owns_executing_stage: false,
                ..blocking()
            },
        ),
    ];
    for (what, facts) in cases {
        assert_eq!(decide("s1", &facts), None, "{what}: the stop was blocked");
    }
}
