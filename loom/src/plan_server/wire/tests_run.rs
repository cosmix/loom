//! Parity tests for run wire types and web fixtures.

use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;

use super::run::{
    Backend, OutputStream, RunJob, RunOffer, RunOutput, RunStateView, RunStrip, WorkspaceState,
};

const STATE: &str = include_str!("../../../../web/src/plan/api/fixtures/run-state.json");
const STRIP: &str = include_str!("../../../../web/src/plan/api/fixtures/run-strip.json");
const OUTPUT: &str = include_str!("../../../../web/src/plan/api/fixtures/run-output.json");
const SCHEMA: &str = include_str!("../../../../web/src/plan/api/schema-run.ts");

fn assert_fixture<T: DeserializeOwned + Serialize>(fixture: &str) {
    let expected: Value = serde_json::from_str(fixture).unwrap();
    let parsed: T = serde_json::from_value(expected.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), expected);
}

#[test]
fn run_state_fixture_matches_serde_output() {
    assert_fixture::<RunStateView>(STATE);
}

#[test]
fn run_state_fixture_deserializes() {
    let state: RunStateView = serde_json::from_str(STATE).unwrap();
    assert_eq!(state.workspace, WorkspaceState::Stopped);
    assert_eq!(state.offer, RunOffer::Resume);
    assert_eq!(state.abandon.unwrap().unmerged_branches.len(), 1);
}

#[test]
fn run_strip_fixture_matches_serde_output() {
    assert_fixture::<RunStrip>(STRIP);
}

#[test]
fn run_strip_fixture_deserializes() {
    let strip: RunStrip = serde_json::from_str(STRIP).unwrap();
    assert!(strip.daemon_running);
    assert_eq!((strip.merged, strip.total, strip.attention), (1, 3, 0));
}

#[test]
fn run_output_fixture_matches_serde_output() {
    assert_fixture::<RunOutput>(OUTPUT);
}

#[test]
fn run_output_fixture_deserializes() {
    let output: RunOutput = serde_json::from_str(OUTPUT).unwrap();
    assert_eq!(output.job, RunJob::Start);
    assert_eq!(output.stream, Some(OutputStream::Stdout));
    assert!(output.line.is_some());
    assert!(!output.done);
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
fn workspace_and_offer_values_are_in_schema() {
    match WorkspaceState::Absent {
        WorkspaceState::Absent
        | WorkspaceState::Initialized
        | WorkspaceState::Running
        | WorkspaceState::Stopped
        | WorkspaceState::Done => {}
    }
    match RunOffer::Start {
        RunOffer::Start
        | RunOffer::StartInitialized
        | RunOffer::NoneRunning
        | RunOffer::Resume
        | RunOffer::StartNext => {}
    }
    assert_all_in_schema(&[
        WorkspaceState::Absent,
        WorkspaceState::Initialized,
        WorkspaceState::Running,
        WorkspaceState::Stopped,
        WorkspaceState::Done,
    ]);
    assert_all_in_schema(&[
        RunOffer::Start,
        RunOffer::StartInitialized,
        RunOffer::NoneRunning,
        RunOffer::Resume,
        RunOffer::StartNext,
    ]);
}

#[test]
fn backend_job_and_stream_values_are_in_schema() {
    match Backend::Tmux {
        Backend::Tmux | Backend::Native => {}
    }
    match RunJob::Commit {
        RunJob::Commit
        | RunJob::Start
        | RunJob::Abandon
        | RunJob::FinishAbandon
        | RunJob::Dashboard => {}
    }
    match OutputStream::Stdout {
        OutputStream::Stdout | OutputStream::Stderr => {}
    }
    assert_all_in_schema(&[Backend::Tmux, Backend::Native]);
    assert_all_in_schema(&[
        RunJob::Commit,
        RunJob::Start,
        RunJob::Abandon,
        RunJob::FinishAbandon,
        RunJob::Dashboard,
    ]);
    assert_all_in_schema(&[OutputStream::Stdout, OutputStream::Stderr]);
}
