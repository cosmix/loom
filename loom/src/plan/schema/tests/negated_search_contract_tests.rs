//! Contracts for the `NegatedSearch` criterion hazard: a negated search over a command's output
//! is flagged, also after a directory change; the fail-closed forms plans use are not.

use crate::plan::schema::validation::criterion_hazards::{scan, Hazard};

const NEGATED_RUNNER_SEARCH: &str =
    r#"! cargo test --test maintainability 2>&1 | rg -q "grew from|new violation at""#;
const NEGATED_AFTER_CD: &str = "cd loom && ! cargo test 2>&1 | grep -q FAILED";

#[track_caller]
fn assert_flagged(command: &str) {
    let found = scan(command, false);
    assert!(
        found.contains(&Hazard::NegatedSearch),
        "not flagged as a negated search: {command}\nfound: {found:?}"
    );
}

#[test]
fn a_negated_search_over_command_output_is_flagged() {
    assert_flagged(NEGATED_RUNNER_SEARCH);
    assert_flagged(NEGATED_AFTER_CD);
}

#[test]
fn fail_closed_searches_are_not_flagged() {
    for command in [
        r#"cargo test | rg -q "test result: ok""#,
        "! rg -q TODO src/lib.rs",
        r#"! rg -q "a|b" file"#,
    ] {
        let found = scan(command, false);
        assert!(
            !found.contains(&Hazard::NegatedSearch),
            "a fail-closed search was flagged: {command}\nfound: {found:?}"
        );
    }
    assert_flagged(NEGATED_RUNNER_SEARCH);
}
