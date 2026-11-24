//! Contract: an impact selection whose run had no loopback and printed a
//! refused bind fails the gate with one note naming
//! `sandbox.network.allow_local_binding` under its failure line, and the
//! failure itself is unchanged.

use std::collections::BTreeSet;
use std::path::Path;

use tempfile::TempDir;

use super::accepted::AcceptedImpact;
use super::base::{BaseProbe, NoBaseRunner};
use super::failure::ImpactFailure;
use super::report::{ImpactTestsFailed, Report};
use super::runs::Selection;
use crate::testrun::{registry, TestTarget};
use crate::verify::criteria::{ProbeRun, ProbeRunner};

/// The stage description's NOTE TEXT quoting `line`.
fn note(line: &str) -> String {
    format!(
        "refused bind? this run had no loopback networking (macOS confinement) and printed: \
         {line}; tests that listen or dial on loopback need \
         sandbox.network.allow_local_binding: true in the plan or stage, which an operator \
         sets; the run still fails, and if it fails only because of where completion runs, \
         dispute it"
    )
}

/// A bun-style bind refusal, then cargo output with two of three tests failed.
const STDOUT: &str = "code: \"EADDRINUSE\"\n\
                      running 3 tests\n\
                      test tests::alpha ... FAILED\n\
                      test tests::beta ... FAILED\n\
                      test tests::gamma ... ok\n\n\
                      failures:\n    tests::alpha\n    tests::beta\n\n\
                      test result: FAILED. 1 passed; 2 failed; 0 ignored; 0 measured; \
                      0 filtered out; finished in 0.00s\n";

/// Exits 101 with [`STDOUT`], its loopback refused or not.
struct Refused(bool);

impl ProbeRunner for Refused {
    fn run(&self, _command: &str, _package_dir: &Path) -> anyhow::Result<ProbeRun> {
        let mut run = ProbeRun::finished(STDOUT, "", Some(101));
        run.loopback_refused = self.0;
        Ok(run)
    }
}

/// The gate's message and failures for one cargo selection run by `runner`,
/// judged with nothing accepted and the base unknown.
fn gate(runner: &Refused) -> (String, Vec<ImpactFailure>) {
    let root = TempDir::new().unwrap();
    let mut selection = Selection::new(root.path(), runner, BTreeSet::new());
    let cargo = registry::by_name("cargo-test").unwrap();
    let targets: Vec<TestTarget> = ["tests::alpha", "tests::beta", "tests::gamma"]
        .iter()
        .map(|name| TestTarget {
            file: "src/lib.rs".to_string(),
            name: Some(name.to_string()),
        })
        .collect();
    selection.run_group(Path::new(""), cargo, &targets);
    let base = BaseProbe {
        worktree: root.path(),
        commit: Err("not under test".to_string()),
        runner: &NoBaseRunner,
    };
    let error = Report::judge(selection, &AcceptedImpact::default(), &base)
        .into_outcome("s1")
        .unwrap_err();
    let failed = error
        .downcast_ref::<ImpactTestsFailed>()
        .expect("the gate fails with ImpactTestsFailed");
    (error.to_string(), failed.failures.clone())
}

#[test]
fn impact_failure_message_carries_the_hint() {
    let (hinted, refused) = gate(&Refused(true));
    let (plain, unrefused) = gate(&Refused(false));
    assert_eq!(refused.len(), 1, "{hinted}");
    assert_eq!(unrefused.len(), 1, "{plain}");
    let (failure, other) = (&refused[0], &unrefused[0]);
    assert!(
        !failure.failed_tests.is_empty(),
        "the failure names its failed tests, so its line holds no output excerpt"
    );
    assert_eq!(failure.id, other.id);
    assert_eq!(failure.kind, other.kind);
    assert_eq!(failure.failed_tests, other.failed_tests);

    assert_eq!(hinted.matches("allow_local_binding").count(), 1, "{hinted}");
    let hint = format!("\n    {}", note("code: \"EADDRINUSE\""));
    let under_line = format!("  - {}{hint}", failure.line());
    assert!(hinted.contains(&under_line), "{hinted}");
    let dispute = "loom stage dispute-criteria s1 --field impact-tests --test";
    assert!(hinted.contains(dispute), "{hinted}");
    assert_eq!(
        hinted.replacen(&hint, "", 1),
        ImpactTestsFailed::for_test("s1", refused).to_string(),
        "the hint is one added line"
    );

    assert!(!plain.contains("allow_local_binding"), "{plain}");
    assert!(plain.contains(dispute), "{plain}");
    assert_eq!(
        plain,
        ImpactTestsFailed::for_test("s1", unrefused).to_string(),
        "no hint, the message as before"
    );
}
