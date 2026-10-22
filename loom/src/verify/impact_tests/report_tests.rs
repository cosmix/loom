//! Judging failed selections against the failures the stage's disputes
//! accepted, as the daemon records them and the gate reads them back.

use super::*;
use crate::testrun::{registry, TestTarget};
use crate::verify::criteria::{ProbeRun, ProbeRunner};
use crate::verify::impact_tests::base::BaseRunner;
use crate::verify::impact_tests::ObservedImpact;
use crate::verify::impact_tests::{load_accepted, AcceptedFailure, FailureKind, ImpactFailure};
use crate::verify::review::verdict_records::append_impact_acceptance;
use std::collections::BTreeSet;
use std::path::Path;
use tempfile::TempDir;

const ID: &str = "IT-cargo-test:.";

/// Reports `FAILED` cargo output with two of three tests failing.
struct TwoFail;

impl ProbeRunner for TwoFail {
    fn run(&self, _command: &str, _package_dir: &Path) -> anyhow::Result<ProbeRun> {
        Ok(ProbeRun {
            stdout: "running 3 tests\n\ntest result: FAILED. 1 passed; 2 failed; 0 ignored; 0 \
                     measured; 0 filtered out; finished in 0.00s\n"
                .to_string(),
            stderr: String::new(),
            exit_code: Some(101),
            timed_out: false,
            cached: None,
        })
    }
}

/// Never makes a base runner: these tests leave the base unknown.
struct NoBaseRunner;

impl BaseRunner for NoBaseRunner {
    fn runner_for(&self, _root: &Path) -> Box<dyn ProbeRunner + '_> {
        unreachable!("the base is unknown")
    }
}

fn targets(names: &[&str]) -> Vec<TestTarget> {
    names
        .iter()
        .map(|name| TestTarget {
            file: "src/lib.rs".to_string(),
            name: Some(name.to_string()),
        })
        .collect()
}

fn fails(failed: u64) -> FailureKind {
    FailureKind::Fails {
        failed: Some(failed),
    }
}

/// Dispute 3 of stage `s` accepted [`ID`] failing as `kind` over `tests`, as
/// the daemon records it; returns the record the gate loads.
fn accepted(work: &Path, kind: FailureKind, tests: &[&str]) -> AcceptedImpact {
    let failure = AcceptedFailure {
        id: ID.to_string(),
        kind,
        tests: tests.iter().map(|test| test.to_string()).collect(),
        dispute: 3,
    };
    append_impact_acceptance(work, "s", &[failure]).unwrap();
    load_accepted(work, "s").unwrap()
}

/// Run the cargo selection of `selected` with two failures, then judge it
/// against `accepted` with the base unknown.
fn judged(accepted: &AcceptedImpact, selected: &[&str]) -> Report {
    let root = TempDir::new().unwrap();
    let mut selection = Selection::new(root.path(), &TwoFail, BTreeSet::new());
    let cargo = registry::by_name("cargo-test").unwrap();
    selection.run_group(Path::new(""), cargo, &targets(selected));
    let base = BaseProbe {
        worktree: root.path(),
        commit: Err("not under test".to_string()),
        runner: &NoBaseRunner,
    };
    Report::judge(selection, accepted, &base)
}

/// Only counted test failures compare: a build failure, an unparsed run or an
/// uncounted failure proves nothing, even against an identical one.
#[test]
fn a_failure_covers_only_counted_failures_no_worse() {
    let uncounted = FailureKind::Fails { failed: None };
    let exit = |code| FailureKind::Unparsed {
        exit_code: Some(code),
    };
    assert!(fails(2).covers(&fails(2)));
    assert!(fails(2).covers(&fails(1)));
    assert!(!fails(1).covers(&fails(2)));
    assert!(!uncounted.covers(&fails(5)));
    assert!(!fails(5).covers(&uncounted));
    assert!(!uncounted.covers(&uncounted));
    assert!(!FailureKind::DoesNotBuild.covers(&FailureKind::DoesNotBuild));
    assert!(!FailureKind::DoesNotBuild.covers(&fails(1)));
    assert!(!fails(9).covers(&FailureKind::DoesNotBuild));
    assert!(!exit(2).covers(&exit(2)));
}

/// An accepted failure loom could not count covers only the identical
/// failure, never another kind or a counted one.
#[test]
fn an_accepted_uncounted_failure_covers_only_an_identical_one() {
    let work = TempDir::new().unwrap();
    let uncounted = FailureKind::Fails { failed: None };
    let uncounted_record = accepted(work.path(), uncounted.clone(), &["a"]);
    let standing = |kind: &FailureKind| uncounted_record.standing(ID, kind, &["a".to_string()]);

    assert!(matches!(standing(&uncounted), Standing::Covered(_)));
    assert!(matches!(standing(&fails(1)), Standing::Changed(_)));
    assert!(matches!(
        standing(&FailureKind::DoesNotBuild),
        Standing::Changed(_)
    ));

    let work = TempDir::new().unwrap();
    let unbuilt_record = accepted(work.path(), FailureKind::DoesNotBuild, &["a"]);
    let standing = |kind: &FailureKind| unbuilt_record.standing(ID, kind, &["a".to_string()]);
    assert!(matches!(
        standing(&FailureKind::DoesNotBuild),
        Standing::Covered(_)
    ));
    assert!(matches!(standing(&uncounted), Standing::Changed(_)));
}

/// An accepted failure covers only itself: the same kind and count over the
/// same set of tests. A smaller count, a shrunken selection or another kind
/// is judged again.
#[test]
fn an_accepted_failure_covers_only_an_identical_one() {
    let work = TempDir::new().unwrap();
    let record = accepted(work.path(), fails(5), &["a", "b"]);
    let tests = |names: &[&str]| -> Vec<String> { names.iter().map(|n| n.to_string()).collect() };

    let exact = record.standing(ID, &fails(5), &tests(&["b", "a"]));
    assert!(matches!(exact, Standing::Covered(_)), "{exact:?}");
    for (kind, selected) in [
        (fails(4), &["a", "b"][..]),
        (fails(6), &["a", "b"][..]),
        (fails(5), &["a"][..]),
        (fails(5), &["a", "b", "c"][..]),
        (FailureKind::DoesNotBuild, &["a", "b"][..]),
    ] {
        let standing = record.standing(ID, &kind, &tests(selected));
        assert!(
            matches!(standing, Standing::Changed(_)),
            "{kind:?} over {selected:?}: {standing:?}"
        );
    }
}

/// A record carries the stage code's own output, so its claim may be inflated.
/// Accepted, it excuses only the exact failure it claims: an inflated count
/// covers no real run that differs.
#[test]
fn a_forged_record_once_accepted_covers_nothing_but_its_claim() {
    let forged = ObservedImpact::of(&[ImpactFailure {
        id: ID.to_string(),
        command: "cargo test -- a b".to_string(),
        tests: vec!["a".to_string(), "b".to_string()],
        kind: fails(500),
        base: "it fails at the base as well".to_string(),
        accepted: None,
        output: "500 failed".to_string(),
    }]);
    let work = TempDir::new().unwrap();
    append_impact_acceptance(
        work.path(),
        "s",
        &[AcceptedFailure::of(&forged.failures[0], 3)],
    )
    .unwrap();
    let accepted = load_accepted(work.path(), "s").unwrap();

    let report = judged(&accepted, &["a", "b"]);

    assert_eq!(report.failures.len(), 1, "{:?}", report.outcome.notes);
    let why = report.failures[0].accepted.as_deref().unwrap_or_default();
    assert!(
        why.contains("dispute 3 accepted it when it fails (500 failed); it fails (2 failed) now"),
        "{why}"
    );
}

#[test]
fn an_accepted_failure_is_a_note_while_it_fails_exactly_as_accepted() {
    let work = TempDir::new().unwrap();
    let accepted = accepted(work.path(), fails(2), &["a", "b"]);

    let report = judged(&accepted, &["a", "b"]);

    assert!(report.failures.is_empty(), "{:?}", report.failures);
    let notes = report.outcome.notes.join("\n");
    assert!(
        notes.contains("`IT-cargo-test:.` fails (2 failed), as dispute 3 accepted it"),
        "{notes}"
    );
    assert!(report.into_outcome("s").is_ok());
}

#[test]
fn a_failure_other_than_accepted_fails_again() {
    let work = TempDir::new().unwrap();
    let accepted = accepted(work.path(), fails(1), &["a", "b"]);

    let error = judged(&accepted, &["a", "b"])
        .into_outcome("s")
        .unwrap_err();

    let message = error.to_string();
    assert!(
        message.contains("dispute 3 accepted it when it fails (1 failed); it fails (2 failed) now"),
        "{message}"
    );
}

#[test]
fn an_accepted_selection_that_grew_a_test_is_judged_again() {
    let work = TempDir::new().unwrap();
    let accepted = accepted(work.path(), fails(2), &["a"]);

    let report = judged(&accepted, &["a", "b"]);

    assert_eq!(report.failures.len(), 1);
    let why = report.failures[0].accepted.as_deref().unwrap_or_default();
    assert!(
        why.contains(
            "dispute 3 accepted it (fails (2 failed)) over other selected tests (selected: a)"
        ),
        "{why}"
    );
}

/// With no accepted record, as after a rejected dispute, the failure fails the
/// gate and the message names the exact dispute to file.
#[test]
fn an_unaccepted_failure_names_its_dispute_command() {
    let report = judged(&AcceptedImpact::default(), &["a"]);

    let error = report.into_outcome("s").unwrap_err();

    let message = error.to_string();
    assert!(
        message.contains("IT-cargo-test:.: `cargo test -- a` fails (2 failed) (selected: a)"),
        "{message}"
    );
    // The completion finds the failures to record under its own context.
    let wrapped = error.context("stage completion failed");
    let failed = wrapped.downcast_ref::<ImpactTestsFailed>().unwrap();
    assert_eq!(failed.failures[0].id, ID);
    assert!(message.contains("the stage's base is unknown"), "{message}");
    assert!(
        message.contains(
            "loom stage dispute-criteria s --field impact-tests --test IT-cargo-test:. --reason"
        ),
        "{message}"
    );
}

/// Past the failures loom keeps for a dispute, the message names the ids that
/// cannot be disputed after this run and leaves them out of the command.
#[test]
fn failures_past_the_kept_ones_are_named_as_not_disputable() {
    let failure = |n: usize| ImpactFailure {
        id: format!("IT-pytest:p{n}"),
        command: "pytest a.py".to_string(),
        tests: vec!["a.py".to_string()],
        kind: fails(1),
        base: "it passes at the stage's base 0123abcd".to_string(),
        accepted: None,
        output: String::new(),
    };
    let failures: Vec<ImpactFailure> = (0..=MAX_OBSERVED_FAILURES).map(failure).collect();
    let last = format!("IT-pytest:p{MAX_OBSERVED_FAILURES}");

    let message = failure_message("s", &failures);

    assert!(
        message.contains(&format!("so {last} cannot be disputed")),
        "{message}"
    );
    assert!(!message.contains(&format!("--test {last}")), "{message}");
    assert!(message.contains("--test IT-pytest:p0 "), "{message}");
    assert!(!failure_message("s", &failures[..1]).contains("cannot be disputed"));
}
