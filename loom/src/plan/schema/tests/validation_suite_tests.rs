//! Tests for the full-suite `cargo test` detection and warning.

use super::make_stage;
use crate::plan::schema::types::{AcceptanceCriterion, StageType};
use crate::plan::schema::validation::validate_structural_preflight;
use crate::plan::schema::validation_suite::is_full_suite_run;

const FULL_SUITE_COMMANDS: &[&str] = &[
    "cargo test --manifest-path loom/Cargo.toml --all-targets --no-fail-fast -- --skip a --skip b",
    "cargo test --all-targets",
    "cargo test",
    "cargo test --workspace",
    "cargo test --all-targets -- --skip a",
    "cargo nextest run",
];

const SCOPED_COMMANDS: &[&str] = &[
    r#"cargo test --manifest-path loom/Cargo.toml --all-targets global_config_tier 2>&1 | rg -q "test result: ok""#,
    "cargo test --lib user_config",
    "cargo test --test integration hooks_read_guard",
    "cargo test --manifest-path loom/Cargo.toml -p loom-map",
    "cargo build --all-targets",
    "cargo test -- --ignored",
    "cargo test -- --exact my::test",
    "cargo test --all-targets --no-run",
    "cargo nextest run -E 'test(foo)'",
    "cargo nextest run --lib",
];

#[test]
fn test_is_full_suite_run_detects_unfiltered_runs() {
    for command in FULL_SUITE_COMMANDS {
        assert!(
            is_full_suite_run(command),
            "expected full-suite run for: {command}"
        );
    }
}

#[test]
fn test_is_full_suite_run_ignores_scoped_runs() {
    for command in SCOPED_COMMANDS {
        assert!(
            !is_full_suite_run(command),
            "expected scoped (non-full-suite) run for: {command}"
        );
    }
}

#[test]
fn test_preflight_warns_on_full_suite_run_for_standard_stage() {
    let mut stage = make_stage("stage-1", "Stage One");
    stage.acceptance = vec![AcceptanceCriterion::Simple(
        FULL_SUITE_COMMANDS[0].to_string(),
    )];
    stage.artifacts = vec!["README.md".to_string()];

    let warnings = validate_structural_preflight(&[stage], None);
    assert!(warnings.iter().any(|w| {
        w.contains("Stage 'stage-1'")
            && w.contains("Acceptance criterion #1")
            && w.contains("runs the whole test suite")
    }));
}

#[test]
fn test_preflight_no_warning_for_full_suite_run_on_integration_verify_stage() {
    let mut stage = make_stage("stage-1", "Stage One");
    stage.stage_type = Some(StageType::IntegrationVerify);
    stage.acceptance = vec![AcceptanceCriterion::Simple(
        FULL_SUITE_COMMANDS[0].to_string(),
    )];
    stage.artifacts = vec!["README.md".to_string()];

    let warnings = validate_structural_preflight(&[stage], None);
    assert!(warnings
        .iter()
        .all(|w| !w.contains("runs the whole test suite")));
}

#[test]
fn test_preflight_no_warning_for_filtered_run_on_standard_stage() {
    let mut stage = make_stage("stage-1", "Stage One");
    stage.acceptance = vec![AcceptanceCriterion::Simple(
        "cargo test --lib user_config".to_string(),
    )];
    stage.artifacts = vec!["README.md".to_string()];

    let warnings = validate_structural_preflight(&[stage], None);
    assert!(warnings
        .iter()
        .all(|w| !w.contains("runs the whole test suite")));
}
