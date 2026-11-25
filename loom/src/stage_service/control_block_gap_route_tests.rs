//! The block guard's route for indexed goal-backward gaps (`artifact-<i>`,
//! `wiring-<i>`, `wiring_tests-<i>`), against a stage whose lists have them.

use super::super::agent_tests::{fixture, record_with, STAGE};
use super::super::*;
use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus, SuccessCriteria, WiringCheck, WiringTest};
use tempfile::TempDir;

/// A fixture whose stage has the artifact, wiring and wiring-test entries an
/// indexed goal-backward gap can name.
fn fixture_with_gap_lists() -> (TempDir, Session, Stage) {
    let (temp, session, mut stage) = fixture();
    stage.artifacts = vec!["a.md".into(), "b.md".into()];
    stage.wiring = vec![WiringCheck {
        source: "src/a.rs".into(),
        pattern: "run".into(),
        description: "a runs".into(),
        literal: false,
    }];
    stage.wiring_tests = (0..3)
        .map(|n| WiringTest {
            name: format!("test {n}"),
            command: "true".into(),
            success_criteria: SuccessCriteria::default(),
            description: None,
        })
        .collect();
    let lists = stage.clone();
    crate::verify::transitions::update_stage(STAGE, temp.path(), |saved| {
        saved.artifacts = lists.artifacts;
        saved.wiring = lists.wiring;
        saved.wiring_tests = lists.wiring_tests;
        Ok(())
    })
    .unwrap();
    (temp, session, stage)
}

#[test]
fn an_indexed_goal_gap_refuses_the_block_and_names_its_dispute_route() {
    let (temp, session, stage) = fixture_with_gap_lists();
    let results = [
        ("goal-backward", false),
        ("artifact-1", false),
        ("wiring-0", false),
        ("wiring_tests-2", false),
    ];
    let phase = CompletionPhase::ToolFailed;
    record_with(&temp, &session, &stage, phase, &results);

    let response = handle_agent_block_stage(temp.path(), STAGE, "x").unwrap();

    let Response::Error { message } = response else {
        panic!("expected a refusal, got {response:?}");
    };
    for route in [
        "dispute-criteria stage-blk --field artifacts --criterion-index 1 --reason",
        "dispute-criteria stage-blk --field wiring --criterion-index 0 --reason",
        "dispute-criteria stage-blk --field wiring-tests --criterion-index 2 --reason",
    ] {
        assert!(message.contains(route), "missing {route:?}: {message}");
    }
    assert_eq!(
        load_stage(STAGE, temp.path()).unwrap().status,
        StageStatus::Executing
    );
}

#[test]
fn a_failed_goal_backward_without_an_indexed_gap_still_blocks() {
    let (temp, session, stage) = fixture();
    let results = [("goal-backward", false)];
    record_with(
        &temp,
        &session,
        &stage,
        CompletionPhase::ToolFailed,
        &results,
    );

    let response = handle_agent_block_stage(temp.path(), STAGE, "dead code").unwrap();

    assert!(matches!(response, Response::Ok), "{response:?}");
}
