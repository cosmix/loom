//! The observed record and the ids it keys failures by.

use super::*;
use crate::models::dispute::FAILURE_OUTPUT_MAX_BYTES;
use crate::testrun::registry;
use crate::verify::impact_tests::failure::failure_id;
use crate::verify::impact_tests::FailureKind;
use tempfile::TempDir;

fn failure(id: &str) -> ImpactFailure {
    ImpactFailure {
        id: id.to_string(),
        command: "cargo test -- a".to_string(),
        tests: vec!["a".to_string()],
        kind: FailureKind::Fails { failed: Some(1) },
        base: "it passes at the stage's base 0123abcd".to_string(),
        accepted: None,
        output: "test result: FAILED".to_string(),
    }
}

/// The separator splits an id unambiguously only while no runner name holds
/// it, whatever the package path holds.
#[test]
fn every_runner_name_makes_an_id_that_checks_and_splits_at_its_runner() {
    for adapter in registry::all() {
        let id = failure_id(adapter.name(), Path::new("deps/kit-a:b"));
        check_failure_id(&id).unwrap();
        let rest = id.strip_prefix("IT-").unwrap();
        assert_eq!(rest.split_once(':'), Some((adapter.name(), "deps/kit-a:b")));
    }
    assert_eq!(failure_id("cargo-test", Path::new("")), "IT-cargo-test:.");
}

#[test]
fn a_malformed_id_is_refused() {
    let long = format!("IT-cargo-test:{}", "p".repeat(600));
    for id in [
        "IT-cargo-test-.",
        "cargo-test:.",
        "IT-cargo:.",
        "IT-cargo-test:",
        "IT-cargo-test:a\nb",
        long.as_str(),
    ] {
        assert!(check_failure_id(id).is_err(), "{id:?} was accepted");
    }
}

#[test]
fn a_recorded_failure_is_bounded() {
    let mut huge = failure("IT-cargo-test:.");
    huge.output = format!("{}the end", "x".repeat(3 * FAILURE_OUTPUT_MAX_BYTES));
    huge.tests = (0..500).map(|n| format!("t{n}")).collect();
    huge.base = "b".repeat(10_000);

    let bounded = ObservedImpact::of(&[huge]).failures.remove(0);

    assert_eq!(bounded.output.len(), FAILURE_OUTPUT_MAX_BYTES);
    assert!(bounded.output.ends_with("the end"));
    assert_eq!(bounded.tests.len(), 64);
    assert_eq!(bounded.base.len(), 1024);
}

#[test]
fn a_record_round_trips_and_a_record_of_nothing_clears_it() {
    let work = TempDir::new().unwrap();
    assert_eq!(
        load_observed(work.path(), "s").unwrap(),
        ObservedImpact::default()
    );
    let observed = ObservedImpact::of(&[failure("IT-cargo-test:.")]);

    record_observed(work.path(), "s", &observed).unwrap();
    assert_eq!(load_observed(work.path(), "s").unwrap(), observed);

    record_observed(work.path(), "s", &ObservedImpact::default()).unwrap();
    assert_eq!(
        load_observed(work.path(), "s").unwrap(),
        ObservedImpact::default()
    );
}

/// Text the stage's code wrote loses its control characters; the output
/// keeps its line breaks and tabs.
#[test]
fn a_recorded_failure_is_stripped_of_control_characters() {
    let mut dirty = failure("IT-cargo-test:.");
    dirty.command = "cargo test\u{1b}[2J -- a\n".to_string();
    dirty.base = "passes\r\u{7}".to_string();
    dirty.tests = vec!["a\u{1b}]0;title\u{7}".to_string(), "b\tc".to_string()];
    dirty.accepted = Some("dispute 1\u{0}".to_string());
    dirty.output = "line\u{1b}[31m one\r\n\tline two\u{8}".to_string();

    let clean = ObservedImpact::of(&[dirty]).failures.remove(0);

    assert_eq!(clean.command, "cargo test[2J -- a");
    assert_eq!(clean.base, "passes");
    assert_eq!(clean.tests, ["a]0;title", "bc"]);
    assert_eq!(clean.accepted.as_deref(), Some("dispute 1"));
    assert_eq!(clean.output, "line[31m one\n\tline two");
}

#[test]
fn a_record_loom_would_not_write_is_refused() {
    let named_twice = ObservedImpact::of(&[failure("IT-pytest:."), failure("IT-pytest:.")]);
    let malformed = ObservedImpact::of(&[failure("IT-pytest-.")]);

    for (record, expected) in [(named_twice, "twice"), (malformed, "not an impact-test id")] {
        let error = record.check().unwrap_err().to_string();
        assert!(error.contains(expected), "{error}");
    }
}
