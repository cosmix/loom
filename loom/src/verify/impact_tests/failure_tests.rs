//! How the gate lists one failed selection.

use super::*;

fn failure(failed_tests: &[&str], output: &str) -> ImpactFailure {
    ImpactFailure {
        id: "IT-cargo-test:loom".to_string(),
        command: "cargo test --lib -- a b c".to_string(),
        tests: ["a", "b", "c"].map(String::from).to_vec(),
        selection_digest: String::new(),
        kind: FailureKind::Fails { failed: Some(1) },
        failed_tests: failed_tests.iter().map(|name| name.to_string()).collect(),
        base: "it passes at the stage's base 0123abc".to_string(),
        accepted: None,
        output: output.to_string(),
    }
}

#[test]
fn a_failure_whose_output_names_its_tests_lists_those_and_no_output() {
    let line = failure(&["b"], "test b ... FAILED\nsecret tail\n").line();
    assert_eq!(
        line,
        "IT-cargo-test:loom: `cargo test --lib -- a b c` fails (1 failed) (failed: b); \
         it passes at the stage's base 0123abc"
    );
}

#[test]
fn a_failure_naming_no_test_lists_the_selection_and_the_end_of_its_output() {
    let output = (1..=20)
        .map(|n| format!("line {n}"))
        .collect::<Vec<_>>()
        .join("\n\n");
    let line = failure(&[], &output).line();
    assert!(line.contains("(selected: a, b, c)"), "{line}");
    let excerpt = line
        .split_once("\n    output ends:\n")
        .expect("an excerpt")
        .1;
    let shown: Vec<&str> = excerpt.lines().map(str::trim).collect();
    let expected: Vec<String> = (6..=20).map(|n| format!("line {n}")).collect();
    assert_eq!(shown, expected, "the last {EXCERPT_LINES} non-blank lines");
}

#[test]
fn a_failure_without_output_adds_no_excerpt() {
    let line = failure(&[], " \n\n").line();
    assert!(!line.contains("output ends"), "{line}");
}

#[test]
fn failed_names_past_the_shown_ones_are_counted() {
    let names: Vec<String> = (0..12).map(|n| format!("t{n}")).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let line = failure(&refs, "").line();
    assert!(line.contains("failed: t0, t1,"), "{line}");
    assert!(line.contains("t9, and 2 more)"), "{line}");
}
