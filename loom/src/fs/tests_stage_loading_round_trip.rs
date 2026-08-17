//! Round-trip fidelity test for `extract_stage_definition`, split out of
//! `tests_stage_loading.rs` to keep that file and its functions under the
//! project's line-count ceilings.
//!
//! Every field a `StageDefinition` carries must survive
//! `Stage::from_definition` -> `serialize_stage_to_markdown` ->
//! `extract_stage_definition` unchanged. This is the direct proof that
//! `definition_from_stage` (`loom/src/fs/stage_loading.rs`) is the exact
//! inverse of `Stage::from_definition`.
//!
//! `StageDefinition` does not derive `PartialEq`, so fields are asserted
//! individually rather than via a single struct comparison; nested types
//! that also lack `PartialEq` (`WiringCheck`, `WiringTest`, `DeadCodeCheck`,
//! `StageSandboxConfig`, `RegressionTest`, `CodeReviewConfig`) are asserted
//! by sub-field instead. The assertions are grouped into several focused
//! tests (rather than one large one) to stay under the project's
//! function line-count ceiling; all of them share one fully-populated
//! fixture built by [`build_full_stage_definition`] and round-tripped by
//! [`round_tripped_stage_definition`].

use crate::fs::stage_loading::extract_stage_definition;
use crate::models::stage::{
    DeadCodeCheck, ExecutionMode, Implementer, RegressionTest, Stage, StageType, SuccessCriteria,
    TruthCheck, WiringCheck, WiringTest,
};
use crate::plan::schema::{
    AcceptanceCriterion, CodeReviewConfig, Implementers, StageDefinition, StageSandboxConfig,
};
use crate::verify::serialize_stage_to_markdown;

fn sample_acceptance_criteria() -> Vec<AcceptanceCriterion> {
    vec![
        AcceptanceCriterion::Simple("cargo test".to_string()),
        AcceptanceCriterion::Extended(TruthCheck {
            command: "cargo build".to_string(),
            stdout_contains: vec!["Compiling".to_string()],
            stdout_not_contains: vec!["error".to_string()],
            stderr_empty: Some(true),
            exit_code: Some(0),
            description: Some("build check".to_string()),
        }),
    ]
}

/// Builds the `pre`/`post` `TruthCheck` fixture shared by `before_stage` and
/// `after_stage`.
fn sample_truth_check(phase: &str) -> TruthCheck {
    TruthCheck {
        command: format!("echo {phase}"),
        stdout_contains: vec![phase.to_string()],
        stdout_not_contains: vec![],
        stderr_empty: None,
        exit_code: Some(0),
        description: Some(format!("{phase}-check")),
    }
}

fn sample_wiring_check() -> WiringCheck {
    WiringCheck {
        source: "src/main.rs".to_string(),
        pattern: "fn main".to_string(),
        description: "entry point exists".to_string(),
    }
}

fn sample_wiring_test() -> WiringTest {
    WiringTest {
        name: "smoke test".to_string(),
        command: "cargo run".to_string(),
        success_criteria: SuccessCriteria {
            exit_code: Some(0),
            stdout_contains: vec!["ok".to_string()],
            stdout_not_contains: vec![],
            stderr_contains: vec![],
            stderr_empty: Some(true),
        },
        description: Some("runs cleanly".to_string()),
    }
}

fn sample_dead_code_check() -> DeadCodeCheck {
    DeadCodeCheck {
        command: "cargo build --message-format=json".to_string(),
        fail_patterns: vec!["warning: unused".to_string()],
        ignore_patterns: vec!["allowed_unused".to_string()],
    }
}

fn sample_sandbox_config() -> StageSandboxConfig {
    StageSandboxConfig {
        enabled: Some(false),
        auto_allow: Some(true),
        allow_unsandboxed_escape: Some(false),
        excluded_commands: vec!["rm".to_string()],
        ..Default::default()
    }
}

fn sample_regression_test() -> RegressionTest {
    RegressionTest {
        file: "tests/regression.rs".to_string(),
        must_contain: vec!["fn test_regression".to_string()],
    }
}

fn sample_code_review_config() -> CodeReviewConfig {
    CodeReviewConfig {
        dimensions: vec!["security".to_string()],
        require_all: false,
    }
}

/// Builds a `StageDefinition` with every field set to a non-default value,
/// so the round trip in [`round_tripped_stage_definition`] exercises all of
/// them. Nested composite fields come from the small `sample_*` helpers
/// above, keeping this constructor under the line-count ceiling.
fn build_full_stage_definition() -> StageDefinition {
    StageDefinition {
        id: "round-trip".to_string(),
        name: "Round Trip Stage".to_string(),
        description: Some("exercises every field".to_string()),
        dependencies: vec!["dep-a".to_string(), "dep-b".to_string()],
        parallel_group: Some("group-1".to_string()),
        acceptance: sample_acceptance_criteria(),
        setup: vec!["cargo fetch".to_string()],
        files: vec!["src/lib.rs".to_string()],
        auto_merge: Some(true),
        working_dir: "loom".to_string(),
        stage_type: Some(StageType::Knowledge),
        artifacts: vec!["src/main.rs".to_string()],
        wiring: vec![sample_wiring_check()],
        wiring_tests: vec![sample_wiring_test()],
        dead_code_check: Some(sample_dead_code_check()),
        before_stage: vec![sample_truth_check("pre")],
        after_stage: vec![sample_truth_check("post")],
        context_budget: Some(42),
        sandbox: sample_sandbox_config(),
        execution_mode: Some(ExecutionMode::Team),
        bug_fix: Some(true),
        regression_test: Some(sample_regression_test()),
        model: Some("opus".to_string()),
        reasoning_effort: Some("xhigh".to_string()),
        code_review: Some(sample_code_review_config()),
        ultracode: true,
        implementers: Implementers::new(vec![Implementer::Codex, Implementer::Claude]),
        subagent_timeout_secs: Some(600),
    }
}

/// Runs [`build_full_stage_definition`] through `Stage::from_definition` ->
/// `serialize_stage_to_markdown` -> `extract_stage_definition` and returns
/// `(original, round_tripped)`.
fn round_tripped_stage_definition() -> (StageDefinition, StageDefinition) {
    let def = build_full_stage_definition();
    let stage = Stage::from_definition(&def, "plan-id");
    let content = serialize_stage_to_markdown(&stage).expect("serialize");
    let round_tripped = extract_stage_definition(&content).expect("round trip should parse");
    (def, round_tripped)
}

#[test]
fn test_extract_stage_definition_round_trip_identity_and_dependencies() {
    let (def, round_tripped) = round_tripped_stage_definition();

    assert_eq!(round_tripped.id, def.id);
    assert_eq!(round_tripped.name, def.name);
    assert_eq!(round_tripped.description, def.description);
    assert_eq!(round_tripped.dependencies, def.dependencies);
    assert_eq!(round_tripped.parallel_group, def.parallel_group);
    assert_eq!(round_tripped.acceptance, def.acceptance);
    assert_eq!(round_tripped.setup, def.setup);
    assert_eq!(round_tripped.files, def.files);
    assert_eq!(round_tripped.auto_merge, def.auto_merge);
    assert_eq!(round_tripped.working_dir, def.working_dir);
    assert_eq!(round_tripped.stage_type, def.stage_type);
    assert_eq!(round_tripped.artifacts, def.artifacts);
}

#[test]
fn test_extract_stage_definition_round_trip_artifacts_wiring_and_verification() {
    let (def, round_tripped) = round_tripped_stage_definition();

    assert_eq!(round_tripped.wiring.len(), 1);
    assert_eq!(round_tripped.wiring[0].source, def.wiring[0].source);
    assert_eq!(round_tripped.wiring[0].pattern, def.wiring[0].pattern);
    assert_eq!(
        round_tripped.wiring[0].description,
        def.wiring[0].description
    );

    assert_eq!(round_tripped.wiring_tests.len(), 1);
    assert_eq!(round_tripped.wiring_tests[0].name, def.wiring_tests[0].name);
    assert_eq!(
        round_tripped.wiring_tests[0].command,
        def.wiring_tests[0].command
    );
    assert_eq!(
        round_tripped.wiring_tests[0].success_criteria.exit_code,
        def.wiring_tests[0].success_criteria.exit_code
    );
    assert_eq!(
        round_tripped.wiring_tests[0]
            .success_criteria
            .stdout_contains,
        def.wiring_tests[0].success_criteria.stdout_contains
    );
    assert_eq!(
        round_tripped.wiring_tests[0].description,
        def.wiring_tests[0].description
    );

    let dead_code = round_tripped
        .dead_code_check
        .expect("dead_code_check must survive");
    let expected_dead_code = def.dead_code_check.as_ref().unwrap();
    assert_eq!(dead_code.command, expected_dead_code.command);
    assert_eq!(dead_code.fail_patterns, expected_dead_code.fail_patterns);
    assert_eq!(
        dead_code.ignore_patterns,
        expected_dead_code.ignore_patterns
    );
}

#[test]
fn test_extract_stage_definition_round_trip_policy_fields() {
    let (def, round_tripped) = round_tripped_stage_definition();

    assert_eq!(round_tripped.model, def.model);
    assert_eq!(round_tripped.reasoning_effort, def.reasoning_effort);

    let code_review = round_tripped.code_review.expect("code_review must survive");
    let expected_code_review = def.code_review.as_ref().unwrap();
    assert_eq!(code_review.dimensions, expected_code_review.dimensions);
    assert_eq!(code_review.require_all, expected_code_review.require_all);

    assert_eq!(round_tripped.ultracode, def.ultracode);
    assert_eq!(round_tripped.implementers, def.implementers);
    assert_eq!(
        round_tripped.subagent_timeout_secs,
        def.subagent_timeout_secs
    );
}

#[test]
fn test_extract_stage_definition_round_trip_sandbox_and_context_budget() {
    let (def, round_tripped) = round_tripped_stage_definition();

    assert_eq!(round_tripped.before_stage, def.before_stage);
    assert_eq!(round_tripped.after_stage, def.after_stage);
    assert_eq!(round_tripped.context_budget, def.context_budget);

    assert_eq!(round_tripped.sandbox.enabled, def.sandbox.enabled);
    assert_eq!(round_tripped.sandbox.auto_allow, def.sandbox.auto_allow);
    assert_eq!(
        round_tripped.sandbox.allow_unsandboxed_escape,
        def.sandbox.allow_unsandboxed_escape
    );
    assert_eq!(
        round_tripped.sandbox.excluded_commands,
        def.sandbox.excluded_commands
    );

    assert_eq!(round_tripped.execution_mode, def.execution_mode);
    assert_eq!(round_tripped.bug_fix, def.bug_fix);

    let regression = round_tripped
        .regression_test
        .expect("regression_test must survive");
    let expected_regression = def.regression_test.as_ref().unwrap();
    assert_eq!(regression.file, expected_regression.file);
    assert_eq!(regression.must_contain, expected_regression.must_contain);
}
