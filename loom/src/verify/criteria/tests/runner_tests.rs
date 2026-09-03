//! Tests for acceptance runner

use std::process::Command;

use tempfile::TempDir;

use crate::models::stage::{CommandConfinement, Stage};
use crate::plan::schema::AcceptanceCriterion;
use crate::verify::criteria::config::CriteriaConfig;
use crate::verify::criteria::runner::{run_acceptance, run_acceptance_with_config};
use crate::verify::criteria::CachePolicy;

#[test]
fn test_run_acceptance_empty() {
    let stage = Stage::new("test".to_string(), None);
    let result = run_acceptance(&stage, None).unwrap();

    assert!(result.all_passed());
    assert_eq!(result.results().len(), 0);
}

#[test]
fn test_run_acceptance_all_pass() {
    let mut stage = Stage::new("test".to_string(), None);
    let command = if cfg!(target_family = "unix") {
        "true"
    } else {
        "exit /b 0"
    };
    stage.add_acceptance_criterion(AcceptanceCriterion::Simple(command.to_string()));
    stage.add_acceptance_criterion(AcceptanceCriterion::Simple(command.to_string()));

    let result = run_acceptance(&stage, None).unwrap();

    assert!(result.all_passed());
    assert_eq!(result.results().len(), 2);
    assert_eq!(result.passed_count(), 2);
    assert_eq!(result.failed_count(), 0);
}

#[test]
fn test_run_acceptance_some_fail() {
    let mut stage = Stage::new("test".to_string(), None);

    if cfg!(target_family = "unix") {
        stage.add_acceptance_criterion(AcceptanceCriterion::Simple("true".to_string()));
        stage.add_acceptance_criterion(AcceptanceCriterion::Simple("false".to_string()));
    } else {
        stage.add_acceptance_criterion(AcceptanceCriterion::Simple("exit /b 0".to_string()));
        stage.add_acceptance_criterion(AcceptanceCriterion::Simple("exit /b 1".to_string()));
    }

    let result = run_acceptance(&stage, None).unwrap();

    assert!(!result.all_passed());
    assert_eq!(result.results().len(), 2);
    assert_eq!(result.passed_count(), 1);
    assert_eq!(result.failed_count(), 1);
    assert_eq!(result.failures().len(), 1);
}

fn init_git_repo() -> TempDir {
    let temp = TempDir::new().unwrap();
    let identity = ["-c", "user.name=Test", "-c", "user.email=test@example.com"];
    let run = |args: &[&str]| {
        let mut full = identity.to_vec();
        full.extend_from_slice(args);
        assert!(Command::new("git")
            .args(&full)
            .current_dir(temp.path())
            .status()
            .unwrap()
            .success());
    };
    run(&["init", "-q"]);
    std::fs::write(temp.path().join("file.txt"), "hello\n").unwrap();
    run(&["add", "file.txt"]);
    run(&["commit", "-q", "-m", "init"]);
    temp
}

/// Env var name carrying the out-of-repo marker path into the acceptance
/// command below. Unique to this test — nothing else in the crate reads or
/// writes it.
const CACHE_TEST_MARKER_ENV_VAR: &str = "LOOM_TEST_ACCEPTANCE_CACHE_MARKER";

#[test]
fn test_run_acceptance_caches_pass_and_skips_second_execution() {
    let repo = init_git_repo();
    let work_dir = TempDir::new().unwrap();
    // The marker lives outside the repo so the acceptance directory's tree
    // (and thus the cache key) stays identical across both runs.
    let marker = work_dir.path().join("ran.marker");

    // The marker's absolute path must never appear as a literal token in the
    // command text: `cache::is_cacheable` runs `git check-ignore` on every
    // path-like token found there, and a path outside `repo` makes that call
    // fail ("outside repository", exit 128) — which conservatively refuses
    // to cache the command at all. Routing the write through an env var
    // avoids the problem: the command text then contains no `/`, so no
    // token is path-like to begin with. `CommandConfinement::Inherit` makes
    // sure the spawned shell actually sees that var — the default confined
    // child clears the environment down to a fixed allowlist that this
    // test-only variable is not on.
    // SAFETY: this test is the only reader/writer of this variable name.
    unsafe { std::env::set_var(CACHE_TEST_MARKER_ENV_VAR, &marker) };

    let mut stage = Stage::new("test".to_string(), None);
    stage.sandbox.command_confinement = Some(CommandConfinement::Inherit);
    stage.add_acceptance_criterion(AcceptanceCriterion::Simple(format!(
        "echo x >> \"${CACHE_TEST_MARKER_ENV_VAR}\""
    )));

    // Pin the policy instead of reading it from the environment:
    // `cache_tests::cache_policy_bypass_from_env` sets
    // `LOOM_ACCEPTANCE_CACHE=0` process-wide for its duration, and this test
    // is not `#[serial]`, so `CriteriaConfig::default()` can observe that
    // value when the two overlap and the second run then never hits the cache.
    let config = CriteriaConfig::default()
        .with_cache_dir(work_dir.path())
        .with_cache_policy(CachePolicy::Use);

    let first = run_acceptance_with_config(&stage, Some(repo.path()), &config).unwrap();
    assert!(first.all_passed());
    assert!(!first.results()[0].cached);
    assert_eq!(std::fs::read_to_string(&marker).unwrap().lines().count(), 1);

    let second = run_acceptance_with_config(&stage, Some(repo.path()), &config).unwrap();
    assert!(second.all_passed());
    assert!(second.results()[0].cached);
    // Marker unchanged — the command did not run again.
    assert_eq!(std::fs::read_to_string(&marker).unwrap().lines().count(), 1);

    // SAFETY: restoring this test's own variable; nothing else reads it.
    unsafe { std::env::remove_var(CACHE_TEST_MARKER_ENV_VAR) };
}

#[test]
fn test_run_acceptance_bypass_policy_never_caches() {
    let repo = init_git_repo();
    let work_dir = TempDir::new().unwrap();
    let marker = work_dir.path().join("ran.marker");

    let mut stage = Stage::new("test".to_string(), None);
    stage.add_acceptance_criterion(AcceptanceCriterion::Simple(format!(
        "echo x >> {}",
        marker.display()
    )));

    let config = CriteriaConfig::default()
        .with_cache_dir(work_dir.path())
        .with_cache_policy(CachePolicy::Bypass);

    run_acceptance_with_config(&stage, Some(repo.path()), &config).unwrap();
    run_acceptance_with_config(&stage, Some(repo.path()), &config).unwrap();

    assert_eq!(std::fs::read_to_string(&marker).unwrap().lines().count(), 2);
}
