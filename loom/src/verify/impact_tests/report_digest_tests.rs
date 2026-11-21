//! An accepted failure over a selection larger than a record lists, and the
//! digest that identifies the whole selection.

use super::tests::{accepted, fails, judged, ID};
use crate::testrun::TestTarget;
use crate::verify::impact_tests::{
    load_accepted, selection_digest, AcceptedFailure, AcceptedImpact, ObservedImpact,
};
use crate::verify::review::verdict_records::append_impact_acceptance;
use std::path::Path;
use tempfile::TempDir;

fn numbered(count: usize) -> Vec<String> {
    (0..count).map(|n| format!("suite::case_{n:03}")).collect()
}

/// The accepted record a verdict writes for the first run of `names`, built
/// through the observed record and the verdict's own conversion.
fn accepted_through_observed(work: &Path, names: &[&str]) -> AcceptedImpact {
    let first = judged(&AcceptedImpact::default(), names);
    let observed = ObservedImpact::of(&first.failures);
    let record = AcceptedFailure::of(&observed.failures[0], 3);
    append_impact_acceptance(work, "s", &[record]).unwrap();
    load_accepted(work, "s").unwrap()
}

/// More names than a record lists: the accepted record keeps 64 of them for
/// display and still covers the identical rerun, and only that one.
#[test]
fn an_accepted_failure_over_100_tests_is_a_note_on_an_identical_rerun() {
    let names = numbered(100);
    let selected: Vec<&str> = names.iter().map(String::as_str).collect();
    let work = TempDir::new().unwrap();
    let accepted = accepted_through_observed(work.path(), &selected);
    assert_eq!(accepted.accepted[0].tests.len(), 64);

    let again = judged(&accepted, &selected);
    assert!(again.failures.is_empty(), "{:?}", again.failures);
    let notes = again.outcome.notes.join("\n");
    assert!(notes.contains("as dispute 3 accepted it"), "{notes}");

    let mut changed = selected.clone();
    changed[99] = "suite::renamed";
    let judged_again = judged(&accepted, &changed);
    assert_eq!(judged_again.failures.len(), 1, "{:?}", judged_again.outcome);
    let why = judged_again.failures[0].accepted.as_deref().unwrap();
    assert!(why.contains("over a different selection of tests"), "{why}");
}

/// The digest names the distinct targets: neither order nor repeats matter,
/// the file does (scoped binaries share libtest names), and a field's
/// boundary does.
#[test]
fn the_selection_digest_identifies_distinct_targets_by_file_and_name() {
    let target = |file: &str, name: Option<&str>| TestTarget {
        file: file.to_string(),
        name: name.map(str::to_string),
    };
    let lib = target("src/lib.rs", Some("a::t"));
    let other = target("src/main.rs", Some("a::t"));
    let digest = |targets: &[TestTarget]| selection_digest(targets);

    assert_eq!(
        digest(&[lib.clone(), other.clone()]),
        digest(&[other.clone(), lib.clone(), lib.clone()])
    );
    assert_ne!(
        digest(std::slice::from_ref(&lib)),
        digest(std::slice::from_ref(&other))
    );
    assert_ne!(
        digest(std::slice::from_ref(&lib)),
        digest(&[lib.clone(), other])
    );
    assert_ne!(
        digest(&[target("src/lib.rs", None)]),
        digest(&[target("src/lib.rs", Some(""))])
    );
    assert_ne!(
        digest(&[target("a", Some("bc"))]),
        digest(&[target("ab", Some("c"))])
    );
    assert_ne!(digest(&[]), digest(&[lib]));
}

/// Over a selection past the listed names, a failure that differs in kind or
/// count is judged again, whatever its digest.
#[test]
fn a_differing_failure_kind_over_a_large_selection_is_judged_again() {
    let names = numbered(100);
    let selected: Vec<&str> = names.iter().map(String::as_str).collect();
    let work = TempDir::new().unwrap();
    let accepted = accepted(work.path(), fails(1), &selected);

    let report = judged(&accepted, &selected);

    let why = report.failures[0].accepted.as_deref().unwrap();
    assert!(
        why.contains("accepted it when it fails (1 failed); it fails (2 failed) now"),
        "{why}"
    );
}

/// A record written without a digest loads with an empty one and covers no
/// run: there is no path that matches it by its listed names.
#[test]
fn an_accepted_record_without_a_digest_covers_nothing() {
    let json = format!(
        r#"{{"version":{},"accepted":[{{"id":"{ID}","kind":{{"outcome":"fails","failed":2}},"tests":["a","b"],"dispute":3}}]}}"#,
        crate::verify::review::store::RECORD_VERSION
    );
    let accepted: AcceptedImpact = serde_json::from_str(&json).unwrap();
    assert!(accepted.accepted[0].selection_digest.is_empty());

    let report = judged(&accepted, &["a", "b"]);

    assert_eq!(report.failures.len(), 1, "{:?}", report.outcome.notes);
}
