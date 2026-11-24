//! A failing contract run that had no loopback and printed a refused bind
//! carries the note naming `sandbox.network.allow_local_binding`, once, in the
//! contract's failure string.

use super::tests::{fixture_with, run_check, Fixture};
use super::*;

const ONE_FAIL: &str = include_str!("../../testrun/fixtures/cargo-test/one-fail.stdout");

/// Exits 101 with a refused bind printed before a counted failure.
struct Refusing {
    loopback_refused: bool,
}

impl ProbeRunner for Refusing {
    fn run(&self, _command: &str, _package_dir: &Path) -> Result<ProbeRun> {
        let stdout = format!("code: \"EADDRINUSE\"\n{ONE_FAIL}");
        let mut run = ProbeRun::finished(stdout, "", Some(101));
        run.loopback_refused = self.loopback_refused;
        Ok(run)
    }
}

fn findings(fx: &Fixture, runner: &Refusing) -> ContractCheck {
    let place = (fx.worktree.as_path(), fx.worktree.as_path());
    findings_with(&fx.stage, &fx.work_dir, place, runner, &[]).unwrap()
}

#[test]
fn a_refused_loopback_adds_the_note_to_the_contract_failure() {
    let fx = fixture_with(Vec::new());
    let error = run_check(
        &fx,
        &Refusing {
            loopback_refused: true,
        },
    )
    .unwrap_err()
    .to_string();
    assert_eq!(error.matches("allow_local_binding").count(), 1, "{error}");

    let check = findings(
        &fx,
        &Refusing {
            loopback_refused: true,
        },
    );
    let [verdict] = check.contracts.as_slice() else {
        panic!("one contract: {check:?}");
    };
    let [failure] = verdict.failures.as_slice() else {
        panic!("one failure string: {verdict:?}");
    };
    assert!(failure.contains("allow_local_binding"), "{failure}");
    assert!(failure.contains("fails: `"), "{failure}");
    assert_eq!(verdict.marker, None);
}

#[test]
fn an_available_loopback_adds_no_note() {
    let fx = fixture_with(Vec::new());
    let error = run_check(
        &fx,
        &Refusing {
            loopback_refused: false,
        },
    )
    .unwrap_err()
    .to_string();
    assert!(!error.contains("allow_local_binding"), "{error}");
    assert!(error.contains("fails: `"), "{error}");
}
