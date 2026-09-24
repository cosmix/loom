//! The zero-test guard (DESIGN D10): a v2 criterion whose runner selected no
//! test fails; a v1 criterion keeps passing on its exit code.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tempfile::TempDir;

use crate::models::stage::{AcceptanceCriterion, CommandConfinement, Stage};
use crate::verify::criteria::cache_contract::{CachedCriterionPass, CriterionContract};
use crate::verify::criteria::cache_fingerprint::InputFingerprint;
use crate::verify::criteria::confine::CommandSpec;
use crate::verify::criteria::result::CriterionResult;
use crate::verify::criteria::runner::run_acceptance;
use crate::verify::criteria::AcceptanceResult;

const NO_MATCH_STDOUT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/testrun/fixtures/cargo-test/no-match.stdout"
);

/// A `cargo` stand-in in `dir` that prints the captured no-match run of
/// `cargo test` and exits 0.
fn fake_cargo(dir: &Path) -> PathBuf {
    let cargo = dir.join("cargo");
    let script = format!("#!/bin/sh\ncat '{NO_MATCH_STDOUT}'\n");
    std::fs::write(&cargo, script).unwrap();
    std::fs::set_permissions(&cargo, std::fs::Permissions::from_mode(0o755)).unwrap();
    cargo
}

fn no_match_stage(dir: &Path, plan_version: u32) -> Stage {
    let command = format!(
        "{} test tests::delta_missing -- --exact",
        fake_cargo(dir).display()
    );
    let mut stage = Stage::new("zero-tests".to_string(), None);
    stage.plan_version = plan_version;
    stage.add_acceptance_criterion(AcceptanceCriterion::Simple(command));
    stage
}

#[test]
fn zero_test_criterion_fails_in_v2() {
    let dir = TempDir::new().unwrap();
    let stage = no_match_stage(dir.path(), 2);

    let result = run_acceptance(&stage, Some(dir.path())).unwrap();

    let AcceptanceResult::Failed { results, failures } = result else {
        panic!("a v2 criterion that ran zero tests must fail");
    };
    assert_eq!(results[0].exit_code, Some(0));
    assert!(!results[0].success);
    assert!(
        failures
            .iter()
            .any(|failure| failure.contains("selected zero tests (cargo-test)")),
        "{failures:?}"
    );
}

#[test]
fn zero_test_criterion_passes_in_v1() {
    let dir = TempDir::new().unwrap();
    let stage = no_match_stage(dir.path(), 1);

    let result = run_acceptance(&stage, Some(dir.path())).unwrap();

    assert!(result.all_passed(), "{result:?}");
}

#[test]
fn cached_pass_certifies_zero_test_guard() {
    let criterion = AcceptanceCriterion::Simple("cargo test".to_string());
    let contract = |guard: bool| {
        CriterionContract::new(
            &CommandSpec::shell("cargo test"),
            &criterion,
            Duration::from_secs(1),
            CommandConfinement::Confined,
        )
        .with_zero_test_guard(guard)
    };
    let result = CriterionResult::new(
        "cargo test".to_string(),
        true,
        String::new(),
        String::new(),
        Some(0),
        Duration::from_millis(5),
        false,
    );
    let fingerprint = InputFingerprint {
        digest: "input".to_string(),
        tree_head: "head".to_string(),
    };
    let record = |guard: bool, tests: Option<u64>| {
        let verdict = contract(guard).verdict_with_tests(&result, tests);
        CachedCriterionPass::from_result(
            &contract(guard),
            &fingerprint,
            &result,
            Duration::ZERO,
            verdict,
        )
    };

    assert!(record(true, Some(0)).is_none());
    assert!(record(false, Some(0)).is_some());
    let guarded = record(true, Some(3)).expect("a run of 3 tests is certifiable");
    assert_eq!(guarded.verdict.tests_executed, Some(3));
    assert!(guarded.certifies(&contract(true), &fingerprint));
    let unguarded = record(false, Some(0)).unwrap();
    assert!(!unguarded.certifies(&contract(true), &fingerprint));
}
