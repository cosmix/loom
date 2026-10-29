//! Parity tests for pressure wire types and web fixtures.

use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;

use super::pressure::{
    CodexState, PressureOptions, PressureRunView, PressureStart, PressureStepKind, RunEndKind,
    StepState,
};

const OPTIONS: &str = include_str!("../../../../web/src/plan/api/fixtures/pressure-options.json");
const RUN: &str = include_str!("../../../../web/src/plan/api/fixtures/pressure-run.json");
const SCHEMA: &str = include_str!("../../../../web/src/plan/api/schema-pressure.ts");

fn assert_fixture<T: DeserializeOwned + Serialize>(fixture: &str) {
    let expected: Value = serde_json::from_str(fixture).unwrap();
    let parsed: T = serde_json::from_value(expected.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), expected);
}

#[test]
fn pressure_options_fixture_matches_serde_output() {
    assert_fixture::<PressureOptions>(OPTIONS);
}

#[test]
fn pressure_options_fixture_deserializes() {
    let options: PressureOptions = serde_json::from_str(OPTIONS).unwrap();
    assert!(options.defaults.rounds > 0);
}

#[test]
fn pressure_run_fixture_matches_serde_output() {
    assert_fixture::<PressureRunView>(RUN);
}

#[test]
fn pressure_run_fixture_deserializes() {
    let run: PressureRunView = serde_json::from_str(RUN).unwrap();
    assert_eq!((run.round, run.rounds), (2, 2));
    assert_eq!(run.step, PressureStepKind::Codex);
    assert_eq!(run.steps.len(), 2);
    assert_eq!(run.steps[0].kind, PressureStepKind::ClaudePressure);
    assert_eq!(run.steps[0].state, StepState::Done);
    assert!(run.steps[0].tokens.is_some());
    assert!(!run.steps[0].edits.is_empty());
    assert_eq!(run.steps[1].kind, PressureStepKind::Codex);
    assert_eq!(run.steps[1].state, StepState::Running);
    assert!(run.ended.is_none());
}

#[test]
fn pressure_start_flattens_slots() {
    let options: PressureOptions = serde_json::from_str(OPTIONS).unwrap();
    let start = PressureStart {
        plan: "doc/plans/example.md".into(),
        slots: options.defaults,
    };
    let value = serde_json::to_value(&start).unwrap();
    assert!(value.get("slots").is_none());
    assert_eq!(value["rounds"], start.slots.rounds);
    assert_eq!(
        serde_json::from_value::<PressureStart>(value).unwrap(),
        start
    );
}

fn assert_all_in_schema<T: Serialize + std::fmt::Debug>(values: &[T]) {
    for value in values {
        let wire = serde_json::to_string(value).unwrap();
        assert!(
            SCHEMA.contains(&wire),
            "schema is missing {value:?} ({wire})"
        );
    }
}

#[test]
fn enum_values_are_in_schema() {
    match PressureStepKind::Codex {
        PressureStepKind::ClaudePressure
        | PressureStepKind::Codex
        | PressureStepKind::ClaudeAddress => {}
    }
    match StepState::Running {
        StepState::Running | StepState::Done | StepState::Failed | StepState::Stopped => {}
    }
    match CodexState::Running {
        CodexState::Running
        | CodexState::ReviewWritten
        | CodexState::NoReview
        | CodexState::Failed => {}
    }
    match RunEndKind::Recorded {
        RunEndKind::Recorded
        | RunEndKind::Stopped
        | RunEndKind::Failed
        | RunEndKind::Interrupted => {}
    }
    assert_all_in_schema(&[
        PressureStepKind::ClaudePressure,
        PressureStepKind::Codex,
        PressureStepKind::ClaudeAddress,
    ]);
    assert_all_in_schema(&[
        StepState::Running,
        StepState::Done,
        StepState::Failed,
        StepState::Stopped,
    ]);
    assert_all_in_schema(&[
        CodexState::Running,
        CodexState::ReviewWritten,
        CodexState::NoReview,
        CodexState::Failed,
    ]);
    assert_all_in_schema(&[
        RunEndKind::Recorded,
        RunEndKind::Stopped,
        RunEndKind::Failed,
        RunEndKind::Interrupted,
    ]);
}
