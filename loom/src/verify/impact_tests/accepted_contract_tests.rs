//! Contract `accepted-impact-matches-full-selection`: a failed selection of
//! more tests than a record lists keeps the digest of the whole selection
//! (`selection_digest`) through the observed record, the accepted record a
//! verdict writes, and reloading both. The identical failure over the same
//! 721 tests is then a note, and a selection that differs only beyond the
//! 64 listed names is judged again.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::Result;
use tempfile::TempDir;

use super::base::{BaseProbe, BaseRunner};
use super::report::Report;
use super::runs::Selection;
use super::{
    load_accepted, load_observed, record_observed, selection_digest, AcceptedFailure,
    AcceptedImpact, ImpactFailure, ObservedImpact,
};
use crate::testrun::{registry, TestTarget};
use crate::verify::criteria::{ProbeRun, ProbeRunner};
use crate::verify::review::verdict_records::append_impact_acceptance;

const STAGE: &str = "s1";
const DISPUTE: u32 = 1;

/// Every run fails one counted test, as cargo reports it.
struct OneFails;

impl ProbeRunner for OneFails {
    fn run(&self, _command: &str, _package_dir: &Path) -> Result<ProbeRun> {
        Ok(ProbeRun::finished(
            "running 721 tests\n\ntest result: FAILED. 720 passed; 1 failed; 0 ignored; \
             0 measured; 0 filtered out; finished in 0.01s\n",
            "",
            Some(101),
        ))
    }
}

/// A base nothing runs at: the contract judges acceptance, not the base.
struct NoBase;

impl BaseRunner for NoBase {
    fn runner_for(&self, _root: &Path) -> Box<dyn ProbeRunner + '_> {
        panic!("the base is unknown, so no selection runs there")
    }

    fn provision(&self, _root: &Path) -> Result<(), String> {
        panic!("the base is unknown, so nothing is provisioned there")
    }
}

fn selected(count: usize) -> Vec<TestTarget> {
    (0..count)
        .map(|n| TestTarget {
            file: "src/lib.rs".to_string(),
            name: Some(format!("suite::case_{n:03}")),
        })
        .collect()
}

/// One completion check of the cargo selection of `targets`.
fn judge(root: &Path, targets: &[TestTarget], accepted: &AcceptedImpact) -> Report {
    let mut selection = Selection::new(root, &OneFails, BTreeSet::new());
    let cargo = registry::by_name("cargo-test").unwrap();
    selection.run_group(Path::new(""), cargo, targets);
    let base = BaseProbe {
        worktree: root,
        commit: Err("not compared by this contract".to_string()),
        runner: &NoBase,
    };
    Report::judge(selection, accepted, &base)
}

/// `failures` recorded as observed and reloaded, then the first accepted as
/// a verdict records it and reloaded: the observed failure and the record.
fn accept_through_records(
    work: &Path,
    failures: &[ImpactFailure],
) -> (ImpactFailure, AcceptedImpact) {
    record_observed(work, STAGE, &ObservedImpact::of(failures))
        .expect("record the observed failures");
    let observed = load_observed(work, STAGE).expect("reload the observed failures");
    let failure = observed
        .failures
        .first()
        .cloned()
        .expect("the failure is observed");
    let accepted = AcceptedFailure::of(&failure, DISPUTE);
    append_impact_acceptance(work, STAGE, &[accepted]).expect("record the accepted verdict");
    (
        failure,
        load_accepted(work, STAGE).expect("reload the accepted record"),
    )
}

/// The digest `targets` has, checked to be lowercase hex SHA-256.
fn digest_of(targets: &[TestTarget]) -> String {
    let digest = selection_digest(targets);
    let lowercase_hex = |c: char| c.is_ascii_digit() || ('a'..='f').contains(&c);
    assert!(
        digest.len() == 64 && digest.chars().all(lowercase_hex),
        "the selection digest is lowercase hex SHA-256"
    );
    digest
}

/// The accepted record of the first failing run over `targets`, checked to
/// keep 64 display names and the digest of all of `targets`.
fn accepted_first_run(root: &Path, work: &Path, targets: &[TestTarget]) -> AcceptedImpact {
    let digest = digest_of(targets);
    let first = judge(root, targets, &AcceptedImpact::default());
    assert_eq!(
        first.failures.len(),
        1,
        "an unaccepted failing selection fails the gate"
    );
    let (observed, accepted) = accept_through_records(work, &first.failures);
    assert_eq!(
        observed.tests.len(),
        64,
        "the record lists 64 names for display"
    );
    assert_eq!(
        observed.selection_digest, digest,
        "the observed record keeps the full digest"
    );
    let record = accepted.accepted.first().expect("the verdict is recorded");
    assert_eq!(
        record.selection_digest, digest,
        "the accepted record keeps the full digest"
    );
    accepted
}

#[test]
fn an_accepted_failure_over_721_tests_covers_the_same_failure_next_run() {
    let (root, work) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let targets = selected(721);
    let accepted = accepted_first_run(root.path(), work.path(), &targets);

    let again = judge(root.path(), &targets, &accepted);
    assert!(
        again.failures.is_empty(),
        "the identical failure is not judged again"
    );
    let dispute = format!("dispute {DISPUTE}");
    assert!(
        again
            .outcome
            .notes
            .iter()
            .any(|note| note.contains(&dispute)),
        "the identical failure is a note naming the dispute that accepted it"
    );

    let mut changed = targets.clone();
    changed[700].name = Some("suite::renamed".to_string());
    assert_ne!(
        digest_of(&changed),
        digest_of(&targets),
        "a changed selection has another digest"
    );
    let changed_run = judge(root.path(), &changed, &accepted);
    assert_eq!(
        changed_run.failures.len(),
        1,
        "a selection that differs beyond the 64th name is judged again"
    );
}
