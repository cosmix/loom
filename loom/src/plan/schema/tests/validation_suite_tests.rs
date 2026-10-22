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

const FULL_AND_FILTERED: &[(&str, &str)] = &[
    ("cargo test", "cargo test --lib foo::"),
    ("bunx vitest run", "bunx vitest run src/a.test.ts"),
    ("uv run pytest", "uv run pytest tests/test_a.py"),
    ("bunx jest", "bunx jest src/a.test.ts"),
    ("go test ./...", "go test ./pkg/a"),
];

#[test]
fn test_is_full_suite_run_covers_other_runners() {
    for (full, filtered) in FULL_AND_FILTERED {
        assert!(is_full_suite_run(full), "expected full run for: {full}");
        assert!(
            !is_full_suite_run(filtered),
            "expected filtered run for: {filtered}"
        );
    }
}

fn subsumed_warnings(stage_type: Option<StageType>, commands: &[&str]) -> Vec<String> {
    let mut stage = make_stage("stage-1", "Stage One");
    stage.stage_type = stage_type;
    stage.acceptance = commands
        .iter()
        .map(|c| AcceptanceCriterion::Simple((*c).to_string()))
        .collect();
    stage.artifacts = vec!["README.md".to_string()];
    validate_structural_preflight(&[stage], None)
        .into_iter()
        .filter(|w| w.contains("already covers"))
        .collect()
}

#[test]
fn test_subsumed_filtered_run_warns_in_integration_verify() {
    for (full, filtered) in FULL_AND_FILTERED {
        let warnings = subsumed_warnings(Some(StageType::IntegrationVerify), &[full, filtered]);
        assert_eq!(warnings.len(), 1, "{full} + {filtered}: {warnings:?}");
        assert!(warnings[0].contains("criterion #2 is a filtered"));
        assert!(warnings[0].contains("criterion #1 already covers"));
    }
}

#[test]
fn test_subsumed_filtered_run_ignored_outside_integration_verify_and_across_runners() {
    let commands = ["cargo test", "cargo test --lib foo::"];
    assert!(subsumed_warnings(None, &commands).is_empty());
    let mixed = ["cargo test", "bunx vitest run src/a.test.ts"];
    assert!(subsumed_warnings(Some(StageType::IntegrationVerify), &mixed).is_empty());
}

#[test]
fn test_skip_or_build_config_difference_defeats_subsumption() {
    let cases = [
        ("cargo test -- --skip net_fetch", "cargo test net_fetch"),
        ("cargo test --skip net_fetch", "cargo test net_fetch"),
        ("cargo test", "cargo test --features tls x"),
        ("cargo test", "cargo test --features=tls x"),
        ("cargo test", "cargo test -Ftls x"),
        ("cargo test", "cargo test -pfoo x"),
        ("cargo test", "cargo test --release x"),
        ("cargo test --features a", "cargo test --features b x"),
    ];
    for (full, filtered) in cases {
        let warnings = subsumed_warnings(Some(StageType::IntegrationVerify), &[full, filtered]);
        assert!(warnings.is_empty(), "{full} + {filtered}: {warnings:?}");
    }
    let same = ["cargo test --features tls", "cargo test --features=tls x"];
    let warnings = subsumed_warnings(Some(StageType::IntegrationVerify), &same);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    let plain = ["cargo test", "cargo test --lib foo::"];
    let warnings = subsumed_warnings(Some(StageType::IntegrationVerify), &plain);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
}

#[test]
fn test_filtered_runs_the_full_suite_does_not_execute_are_not_subsumed() {
    let exempt = [
        "cargo test --doc --workspace --all-features -- --test-threads=1",
        "cargo test --test signal_replay -- --ignored",
        "cargo test --lib foo:: -- --include-ignored",
        "LOOM_TEST_REQUIRE_SANDBOX_FREE=1 cargo test --lib tests_capsule_denies::bwrap::",
        "uv run pytest -m slow",
    ];
    for filtered in exempt {
        let full = if filtered.starts_with("uv") {
            "uv run pytest"
        } else {
            "cargo test --all-targets"
        };
        let warnings = subsumed_warnings(Some(StageType::IntegrationVerify), &[full, filtered]);
        assert!(warnings.is_empty(), "{filtered} warned: {warnings:?}");
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

const INTEGRATION_VERIFY: Option<StageType> = Some(StageType::IntegrationVerify);

/// A filtered run that configures the run differently, or builds targets the
/// full run leaves out, covers what the full run does not.
#[test]
fn test_configuration_a_full_run_lacks_defeats_subsumption() {
    let cases = [
        (
            "go test ./...",
            "go test -tags integration ./internal/db/...",
        ),
        ("go test ./...", "go test -race ./pkg"),
        ("go test -tags a ./...", "go test -tags b ./pkg"),
        (
            "uv run pytest",
            "uv run pytest -c other.ini tests/test_a.py",
        ),
        (
            "bunx vitest run",
            "bunx vitest run --config e2e.ts tests/e2e",
        ),
        ("cargo test", "cargo test --benches"),
        ("cargo test", "cargo test --example demo"),
    ];
    for (full, filtered) in cases {
        let warnings = subsumed_warnings(INTEGRATION_VERIFY, &[full, filtered]);
        assert!(warnings.is_empty(), "{full} + {filtered}: {warnings:?}");
    }
}

/// Selection flags and operands filter without configuring: the full run
/// already executes what they select.
#[test]
fn test_selection_flags_and_operands_stay_subsumed() {
    let cases = [
        ("go test ./...", "go test -run TestX ./..."),
        ("uv run pytest", "uv run pytest -k foo"),
        ("go test -race ./...", "go test -race ./pkg"),
        ("go test -tags a ./...", "go test -tags a ./pkg"),
        ("cargo test --all-targets", "cargo test --benches"),
        ("cargo test --all-targets", "cargo test --example demo"),
        ("cargo test --all-targets", "cargo test --all-targets foo::"),
    ];
    for (full, filtered) in cases {
        let warnings = subsumed_warnings(INTEGRATION_VERIFY, &[full, filtered]);
        assert_eq!(warnings.len(), 1, "{full} + {filtered}: {warnings:?}");
    }
}

#[test]
fn test_attached_package_scoping_is_not_a_full_suite_run() {
    for command in ["cargo test -pfoo", "cargo test -p foo", "cargo test -p=foo"] {
        assert!(!is_full_suite_run(command), "{command} scopes a package");
    }
}
