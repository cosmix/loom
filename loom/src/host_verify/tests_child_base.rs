//! A check at the stage's base (`child_base`): each selected criterion runs in
//! a checkout of the merge base kept beside the stage cache, becomes an outcome
//! `base:<index>`, and is recorded as base evidence. Tests that run criteria
//! start loom's real confinement and skip where it cannot start.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serial_test::serial;

use super::child::Progress;
use super::child_base::{base_config, base_dirs, describe, evidence, parsed};
use super::tests_child::{confinement_unavailable, git, Fixture, STAGE};
use super::{store, JobRecord, JobState};
use crate::models::stage::StageType;
use crate::verify::review::handoffs::{load_base_evidence, BaseEvidence};

/// A criterion run through a `pytest` that only prints `summary` and exits
/// `code`: the pytest adapter reads its counts.
const CRITERION: &str = "PATH=\"$PWD/bin:$PATH\" pytest -q";

/// The stage of `Fixture`, whose base commit holds a fake `pytest` printing
/// `summary` and exiting `code`, while the worktree's own copy always passes:
/// a run that reads the worktree would pass whatever the base does.
struct BaseFixture {
    fx: Fixture,
    base: String,
}

impl BaseFixture {
    fn new(summary: &str, code: i32) -> Self {
        let fx = Fixture::new(&[CRITERION], StageType::Standard);
        let repo = fx
            .work_dir
            .parent()
            .and_then(Path::parent)
            .unwrap()
            .to_path_buf();
        fs::write(repo.join(".gitignore"), ".loom/\n.worktrees/\nbuild/\n").unwrap();
        write_pytest(&repo, summary, code);
        git(&repo, &["add", ".gitignore", "bin/pytest"]);
        git(&repo, &["commit", "-q", "-m", "base"]);
        let base = git(&repo, &["rev-parse", "HEAD"]);
        git(&fx.worktree, &["merge", "-q", "--ff-only", "main"]);
        write_pytest(&fx.worktree, "3 passed in 0.01s", 0);
        git(&fx.worktree, &["commit", "-q", "-am", "stage work"]);
        Self { fx, base }
    }

    /// Save and run a running at-base job of criterion 0.
    fn run(&self) -> super::JobResult {
        let record = JobRecord {
            criteria: Some(vec![0]),
            at_base: true,
            ..self.fx.job(JobState::Running, false)
        };
        store::save_current(&self.fx.work_dir, &record).unwrap();
        self.fx.run(&record)
    }

    fn checkout(&self) -> PathBuf {
        base_dirs(&self.fx.work_dir, STAGE, &self.base)
            .unwrap()
            .checkout
    }
}

fn write_pytest(root: &Path, summary: &str, code: i32) {
    let bin = root.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let script = bin.join("pytest");
    fs::write(
        &script,
        format!("#!/bin/sh\necho '=== {summary} ==='\nexit {code}\n"),
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
#[serial]
fn a_criterion_failing_at_the_base_records_its_counts() {
    if confinement_unavailable("a_criterion_failing_at_the_base_records_its_counts") {
        return;
    }
    let bfx = BaseFixture::new("1 failed, 2 passed in 0.01s", 1);
    let result = bfx.run();
    assert!(!result.passed, "{result:?}");
    assert_eq!(result.error, None);
    let outcome = &result.criteria[0];
    assert_eq!(outcome.label, "base:0");
    assert_eq!(
        outcome.summary,
        format!("at base {}: failed 1 of 3", &bfx.base[..12])
    );
    let evidence = load_base_evidence(&bfx.fx.work_dir, STAGE).unwrap();
    assert_eq!(
        evidence,
        [BaseEvidence {
            index: 0,
            command: CRITERION.to_string(),
            base: bfx.base.clone(),
            passed: false,
            executed: Some(3),
            failed: Some(1),
            build_failed: false,
            timed_out: false,
        }]
    );
}

#[test]
#[serial]
fn a_criterion_passing_at_the_base_records_a_pass() {
    if confinement_unavailable("a_criterion_passing_at_the_base_records_a_pass") {
        return;
    }
    let bfx = BaseFixture::new("2 passed in 0.01s", 0);
    let result = bfx.run();
    assert!(result.passed, "{result:?}");
    let evidence = load_base_evidence(&bfx.fx.work_dir, STAGE).unwrap();
    assert_eq!(evidence.len(), 1, "{evidence:?}");
    assert!(evidence[0].passed);
    assert_eq!(
        (evidence[0].executed, evidence[0].failed),
        (Some(2), Some(0))
    );
}

#[test]
#[serial]
fn a_second_run_reuses_the_checkout_and_its_build() {
    if confinement_unavailable("a_second_run_reuses_the_checkout_and_its_build") {
        return;
    }
    let bfx = BaseFixture::new("1 failed, 2 passed in 0.01s", 1);
    bfx.run();
    let checkout = bfx.checkout();
    let built = checkout.join("build").join("artifact");
    fs::create_dir_all(built.parent().unwrap()).unwrap();
    fs::write(&built, "warm").unwrap();
    let again = bfx.run();
    assert_eq!(again.error, None, "{again:?}");
    assert!(built.exists(), "the ignored build output stays");
    assert_eq!(
        load_base_evidence(&bfx.fx.work_dir, STAGE).unwrap().len(),
        1
    );
}

#[test]
#[serial]
fn a_checkout_with_a_foreign_head_is_made_again() {
    if confinement_unavailable("a_checkout_with_a_foreign_head_is_made_again") {
        return;
    }
    let bfx = BaseFixture::new("1 failed, 2 passed in 0.01s", 1);
    bfx.run();
    let checkout = bfx.checkout();
    let foreign = git(&bfx.fx.worktree, &["rev-parse", "HEAD"]);
    fs::write(checkout.join(".git").join("HEAD"), format!("{foreign}\n")).unwrap();
    let built = checkout.join("build").join("artifact");
    fs::create_dir_all(built.parent().unwrap()).unwrap();
    fs::write(&built, "stale").unwrap();
    let again = bfx.run();
    assert_eq!(again.error, None, "{again:?}");
    assert!(!built.exists(), "the checkout was deleted and made again");
    let head = fs::read_to_string(checkout.join(".git").join("HEAD")).unwrap();
    assert_eq!(head.trim(), bfx.base);
    assert_eq!(
        again.criteria[0].summary,
        format!("at base {}: failed 1 of 3", &bfx.base[..12])
    );
}

#[test]
#[serial]
fn a_base_run_builds_with_the_base_cache_and_never_the_stage_cache() {
    use crate::stage_cache::Consumer;
    let fx = Fixture::new(&["true"], StageType::Standard);
    let stage = crate::verify::transitions::load_stage(STAGE, &fx.work_dir).unwrap();
    let base = "0123456789abcdef0123456789abcdef01234567";
    let dirs = base_dirs(&fx.work_dir, STAGE, base).unwrap();
    let progress = std::sync::Arc::new(Progress::new(&fx.work_dir, STAGE, "job"));
    let config = base_config(&stage, &dirs.checkout, &dirs.cache, &fx.work_dir, &progress).unwrap();
    let granted = config
        .stage_cache
        .expect("a confined base run has a build cache");
    assert_eq!(granted, dirs.cache);
    assert!(
        config.cache_dir.is_none(),
        "a base run proves nothing about the worktree"
    );
    let stage_root = crate::verify::criteria::stage_cache_for(&fx.work_dir, STAGE)
        .unwrap()
        .ensure()
        .unwrap();
    assert_eq!(
        granted.ensure().unwrap(),
        granted.root(),
        "the grant is the base root"
    );
    assert!(
        !granted.root().starts_with(&stage_root),
        "{}",
        granted.root().display()
    );
    let host = std::collections::BTreeMap::new();
    for (name, value) in granted.environment(Consumer::HostJob, &dirs.checkout, &host, None) {
        let value = Path::new(&value);
        assert!(
            !value.starts_with(&stage_root),
            "{name} names the stage cache"
        );
        if name.ends_with("_DIR") || name == "SCCACHE_SERVER_UDS" {
            assert!(
                value.starts_with(granted.root()),
                "{name}={}",
                value.display()
            );
        }
    }
}

#[test]
fn at_base_needs_a_criterion_and_no_contracts() {
    use crate::commands::verify::{check_selection, CheckArgs, CheckSelection};
    let args = |criterion: Vec<usize>, contracts: bool| CheckArgs {
        no_cache: false,
        criterion,
        contracts,
        replacement: Vec::new(),
        at_base: true,
    };
    let refused = check_selection(&args(Vec::new(), false)).unwrap_err();
    assert!(refused.to_string().contains("--criterion"), "{refused}");
    assert!(check_selection(&args(vec![0], true)).is_err());
    assert_eq!(
        check_selection(&args(vec![2, 0, 2], false)).unwrap(),
        CheckSelection::AtBase(vec![0, 2])
    );
}

#[test]
fn each_base_result_has_its_summary() {
    let evidence = |passed, executed, failed, build_failed, timed_out| BaseEvidence {
        index: 0,
        command: "c".to_string(),
        base: "b".to_string(),
        passed,
        executed,
        failed,
        build_failed,
        timed_out,
    };
    assert_eq!(
        describe(&evidence(true, Some(2), Some(0), false, false)),
        "passed"
    );
    assert_eq!(
        describe(&evidence(false, Some(3), Some(1), false, false)),
        "failed 1 of 3"
    );
    assert_eq!(
        describe(&evidence(false, None, None, false, false)),
        "failed"
    );
    assert_eq!(
        describe(&evidence(false, Some(0), None, false, false)),
        "selected zero tests"
    );
    assert_eq!(
        describe(&evidence(false, None, None, true, false)),
        "did not build"
    );
    assert_eq!(
        describe(&evidence(false, None, None, false, true)),
        "timed out"
    );
}

#[test]
fn the_adapter_counts_and_build_failures_become_evidence() {
    let run = |stdout: &str, code: i32| {
        crate::verify::criteria::CriterionResult::new(
            CRITERION.to_string(),
            code == 0,
            stdout.to_string(),
            String::new(),
            Some(code),
            std::time::Duration::ZERO,
            false,
        )
    };
    let cwd = Path::new(".");
    let failing = run("=== 1 failed, 2 passed in 0.01s ===", 1);
    let recorded = evidence(
        3,
        CRITERION,
        "b",
        &failing,
        parsed(CRITERION, cwd, &failing),
    );
    assert_eq!(recorded.base, "b");
    assert_eq!(
        (
            recorded.index,
            recorded.passed,
            recorded.executed,
            recorded.failed
        ),
        (3, false, Some(3), Some(1))
    );
    assert!(!recorded.build_failed);
    let broken = run(
        "=== 1 error in 0.01s ===\nERROR collecting tests/test_a.py",
        2,
    );
    let recorded = evidence(0, CRITERION, "b", &broken, parsed(CRITERION, cwd, &broken));
    assert!(recorded.build_failed, "{recorded:?}");
    let unknown = run("ok", 0);
    let recorded = evidence(
        0,
        "make check",
        "b",
        &unknown,
        parsed("make check", cwd, &unknown),
    );
    assert_eq!((recorded.executed, recorded.failed), (None, None));
    assert!(recorded.passed && !recorded.build_failed);
}
