use crate::plan::schema::tests::{create_valid_metadata, make_stage};
use crate::plan::schema::types::AcceptanceCriterion;
use crate::plan::schema::validation::criterion_hazards::{scan, Hazard};
use crate::plan::schema::validation::{validate, validate_structural_preflight};

const NEGATED_SEARCH: &str = r#"! cargo test --test maintainability 2>&1 | rg -q "grew from|new violation at|scan failed|invalid maintainability baseline|could not compile""#;

#[track_caller]
fn assert_flags(command: &str, hazard: Hazard) {
    let found = scan(command, false);
    assert_eq!(found.into_iter().collect::<Vec<_>>(), [hazard], "{command}");
}

#[track_caller]
fn assert_clean(command: &str) {
    let found = scan(command, false);
    assert!(found.is_empty(), "hazards in `{command}`: {found:?}");
}

#[test]
fn warns_on_negated_pipelines_to_matchers() {
    for command in [
        NEGATED_SEARCH,
        "cd loom && ! cargo test 2>&1 | grep -q FAILED",
        "! make lint |& rg -q warning",
        "cd loom || ! cargo test | /usr/bin/egrep -q FAILED",
        "! (cargo build || cargo test) | cat | fgrep -q FAILED",
        "(! cargo test | ag -q FAILED);",
    ] {
        assert_flags(command, Hazard::NegatedSearch);
    }
}

#[test]
fn warns_when_the_final_test_accepts_a_matcher_failure() {
    for command in [
        r#"cargo test 2>&1 | rg -q "grew from"; test $? -eq 1"#,
        "cargo build 2>&1 | grep -c error; [ $? -ne 0 ]",
        "cd loom && cargo build | grep -c error; test \"$?\" -ne 0",
        "cargo build | rg -q error\n[ $? -eq 1 ]",
    ] {
        assert_flags(command, Hazard::NegatedSearch);
    }
}

#[test]
fn ignores_searches_outside_the_two_shapes() {
    for command in [
        r#"cargo test | rg -q "test result: ok""#,
        "! rg -q TODO src/lib.rs",
        "rg -q -F x file; test $? -eq 1",
        r#"! rg -q "a|b" file"#,
        "! cargo test",
        "'!' cargo test | rg -q FAILED",
        "! cargo build | cat",
        "! cargo test | rg -q FAILED; echo done",
        "! cargo test | rg -q FAILED && echo done",
        "cargo test | rg -q FAILED && echo done; test $? -eq 1",
        "cargo test | rg -q FAILED; test '$?' -eq 1",
        "((! cargo test | cat) | rg -q FAILED)",
    ] {
        assert_clean(command);
    }
}

#[test]
fn forced_success_still_only_flags_masked_exit() {
    assert_flags("cargo test || true", Hazard::MaskedExit);
}

#[test]
fn negated_search_is_a_preflight_warning() {
    let mut stage = make_stage("feature", "Feature");
    stage.acceptance = vec![AcceptanceCriterion::Simple(NEGATED_SEARCH.to_string())];
    let mut metadata = create_valid_metadata();
    metadata.loom.stages = vec![stage];

    let warnings = validate_structural_preflight(&metadata.loom.stages, None);
    assert!(
        warnings.iter().any(|warning| {
            warning.contains("Acceptance criterion #1") && warning.contains("negates a search")
        }),
        "{warnings:?}"
    );
    let result = validate(&metadata);
    assert!(result.is_ok(), "{result:?}");
}
