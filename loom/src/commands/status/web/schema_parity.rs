//! Parity check: every wire value of the status-relevant enums must appear
//! in the web dashboard's zod schema (`web/src/api/schema.ts`), so a variant
//! added on the Rust side cannot silently make the dashboard reject every
//! snapshot frame it receives.
//!
//! Each `all_*` function below opens with an exhaustive `match` (no wildcard
//! arm) over a placeholder instance of the enum; adding, removing, or
//! renaming a variant fails that match to compile until the list right below
//! it is updated by hand to match.

use crate::models::failure::FailureType;
use crate::models::session::{SessionExitReason, SessionType};
use crate::models::stage::{StageStatus, StageType};

const SCHEMA_TS: &str = include_str!("../../../../../web/src/api/schema.ts");

fn all_session_types() -> Vec<SessionType> {
    match SessionType::Stage {
        SessionType::Stage
        | SessionType::Merge
        | SessionType::BaseConflict
        | SessionType::Knowledge
        | SessionType::Adjudication
        | SessionType::Contract => {}
    }
    vec![
        SessionType::Stage,
        SessionType::Merge,
        SessionType::BaseConflict,
        SessionType::Knowledge,
        SessionType::Adjudication,
        SessionType::Contract,
    ]
}

fn all_stage_statuses() -> Vec<StageStatus> {
    match StageStatus::WaitingForDeps {
        StageStatus::WaitingForDeps
        | StageStatus::Queued
        | StageStatus::Executing
        | StageStatus::WaitingForInput
        | StageStatus::Blocked
        | StageStatus::Completed
        | StageStatus::NeedsHandoff
        | StageStatus::Skipped
        | StageStatus::MergeConflict
        | StageStatus::CompletedWithFailures
        | StageStatus::MergeBlocked
        | StageStatus::NeedsHumanReview
        | StageStatus::NeedsAdjudication => {}
    }
    vec![
        StageStatus::WaitingForDeps,
        StageStatus::Queued,
        StageStatus::Executing,
        StageStatus::WaitingForInput,
        StageStatus::Blocked,
        StageStatus::Completed,
        StageStatus::NeedsHandoff,
        StageStatus::Skipped,
        StageStatus::MergeConflict,
        StageStatus::CompletedWithFailures,
        StageStatus::MergeBlocked,
        StageStatus::NeedsHumanReview,
        StageStatus::NeedsAdjudication,
    ]
}

fn all_stage_types() -> Vec<StageType> {
    match StageType::Standard {
        StageType::Standard
        | StageType::Knowledge
        | StageType::IntegrationVerify
        | StageType::KnowledgeDistill => {}
    }
    vec![
        StageType::Standard,
        StageType::Knowledge,
        StageType::IntegrationVerify,
        StageType::KnowledgeDistill,
    ]
}

fn all_failure_types() -> Vec<FailureType> {
    match FailureType::SessionCrash {
        FailureType::SessionCrash
        | FailureType::ContextExhausted
        | FailureType::TestFailure
        | FailureType::BuildFailure
        | FailureType::CodeError
        | FailureType::Timeout
        | FailureType::UserBlocked
        | FailureType::MergeConflict
        | FailureType::InfrastructureError
        | FailureType::SandboxSetupFailure
        | FailureType::StartupRefusal
        | FailureType::Unknown => {}
    }
    vec![
        FailureType::SessionCrash,
        FailureType::ContextExhausted,
        FailureType::TestFailure,
        FailureType::BuildFailure,
        FailureType::CodeError,
        FailureType::Timeout,
        FailureType::UserBlocked,
        FailureType::MergeConflict,
        FailureType::InfrastructureError,
        FailureType::SandboxSetupFailure,
        FailureType::StartupRefusal,
        FailureType::Unknown,
    ]
}

fn all_session_exit_reasons() -> Vec<SessionExitReason> {
    match SessionExitReason::Completed {
        SessionExitReason::Completed
        | SessionExitReason::Crashed
        | SessionExitReason::ContextCeiling
        | SessionExitReason::Stalled
        | SessionExitReason::OperatorStop
        | SessionExitReason::CriteriaBlocked
        | SessionExitReason::Replaced => {}
    }
    vec![
        SessionExitReason::Completed,
        SessionExitReason::Crashed,
        SessionExitReason::ContextCeiling,
        SessionExitReason::Stalled,
        SessionExitReason::OperatorStop,
        SessionExitReason::CriteriaBlocked,
        SessionExitReason::Replaced,
    ]
}

/// Assert every serialized value in `values` appears in `SCHEMA_TS` as a
/// double-quoted string literal.
fn assert_all_in_schema<T: serde::Serialize + std::fmt::Debug>(values: &[T], enum_name: &str) {
    for value in values {
        let serialized = serde_json::to_value(value)
            .unwrap_or_else(|error| panic!("failed to serialize {enum_name} {value:?}: {error}"));
        let wire = serialized
            .as_str()
            .unwrap_or_else(|| panic!("{enum_name} {value:?} did not serialize to a string"));
        let needle = format!("\"{wire}\"");
        assert!(
            SCHEMA_TS.contains(&needle),
            "web/src/api/schema.ts is missing {enum_name} variant {value:?} (wire value {wire:?}); add {needle} to its zod enum"
        );
    }
}

#[test]
fn session_type_wire_values_are_in_web_schema() {
    assert_all_in_schema(&all_session_types(), "SessionType");
}

#[test]
fn stage_status_wire_values_are_in_web_schema() {
    assert_all_in_schema(&all_stage_statuses(), "StageStatus");
}

#[test]
fn stage_type_wire_values_are_in_web_schema() {
    assert_all_in_schema(&all_stage_types(), "StageType");
}

#[test]
fn failure_type_wire_values_are_in_web_schema() {
    assert_all_in_schema(&all_failure_types(), "FailureType");
}

#[test]
fn session_exit_reason_wire_values_are_in_web_schema() {
    assert_all_in_schema(&all_session_exit_reasons(), "SessionExitReason");
}
