//! Full-verdict cache tests for the acceptance runner.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serial_test::serial;
use tempfile::TempDir;

use crate::models::stage::{AcceptanceCriterion, CommandConfinement, Stage, TruthCheck};
use crate::verify::criteria::config::CriteriaConfig;
use crate::verify::criteria::runner::{run_acceptance, run_acceptance_with_config};
use crate::verify::criteria::{AcceptanceResult, CachePolicy};

const TEST_IDENTITY: &[&str] = &["-c", "user.name=Test", "-c", "user.email=test@example.com"];

struct Fixture {
    repo: TempDir,
    cache: TempDir,
    marker: PathBuf,
    script: PathBuf,
}

impl Fixture {
    fn new(body: &str) -> Self {
        let repo = TempDir::new().unwrap();
        let cache = TempDir::new().unwrap();
        let marker = cache.path().join("executions.log");
        let script = repo.path().join("scripts/probe.sh");
        std::fs::create_dir_all(script.parent().unwrap()).unwrap();
        git(&["init", "-q"], repo.path());
        write_probe(&script, &marker, body);
        std::fs::write(repo.path().join("source.txt"), "source\n").unwrap();
        git(&["add", "scripts/probe.sh", "source.txt"], repo.path());
        git(&["commit", "-q", "-m", "fixture"], repo.path());
        Self {
            repo,
            cache,
            marker,
            script,
        }
    }

    fn config(&self, timeout: Duration) -> CriteriaConfig {
        CriteriaConfig::with_timeout(timeout)
            .with_cache_dir(self.cache.path())
            .with_cache_policy(CachePolicy::Use)
    }

    fn executions(&self) -> usize {
        std::fs::read_to_string(&self.marker)
            .unwrap_or_default()
            .lines()
            .count()
    }

    fn mutate_script(&self) {
        let mut content = std::fs::read_to_string(&self.script).unwrap();
        content.push_str("# fingerprint mutation\n");
        std::fs::write(&self.script, content).unwrap();
    }
}

fn write_probe(script: &Path, marker: &Path, body: &str) {
    let content = format!(
        "#!/bin/sh\nprintf 'run\\n' >> '{}'\n{body}\n",
        marker.display()
    );
    std::fs::write(script, content).unwrap();
}

fn git(args: &[&str], dir: &Path) {
    let mut full = TEST_IDENTITY.to_vec();
    full.extend_from_slice(args);
    let status = Command::new("git")
        .args(&full)
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed in {}", dir.display());
}

fn simple_stage(command: &str) -> Stage {
    let mut stage = Stage::new("cache-test".to_string(), None);
    stage.add_acceptance_criterion(AcceptanceCriterion::Simple(command.to_string()));
    stage
}

fn extended_stage(
    command: &str,
    contains: &[&str],
    not_contains: &[&str],
    stderr_empty: bool,
    exit_code: i32,
) -> Stage {
    let mut stage = Stage::new("cache-test".to_string(), None);
    stage.add_acceptance_criterion(AcceptanceCriterion::Extended(TruthCheck {
        command: command.to_string(),
        stdout_contains: contains.iter().map(|value| (*value).to_string()).collect(),
        stdout_not_contains: not_contains
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        stderr_empty: stderr_empty.then_some(true),
        exit_code: Some(exit_code),
        description: None,
    }));
    stage
}

fn run(fixture: &Fixture, stage: &Stage, config: &CriteriaConfig) -> AcceptanceResult {
    run_acceptance_with_config(stage, Some(fixture.repo.path()), config).unwrap()
}

#[test]
fn empty_and_mixed_uncached_stages_keep_existing_verdicts() {
    let empty = Stage::new("empty".to_string(), None);
    assert!(run_acceptance(&empty, None).unwrap().all_passed());

    let mut mixed = Stage::new("mixed".to_string(), None);
    let (pass, fail) = if cfg!(target_family = "unix") {
        ("true", "false")
    } else {
        ("exit /b 0", "exit /b 1")
    };
    mixed.add_acceptance_criterion(AcceptanceCriterion::Simple(pass.to_string()));
    mixed.add_acceptance_criterion(AcceptanceCriterion::Simple(fail.to_string()));
    let result = run_acceptance(&mixed, None).unwrap();
    assert!(!result.all_passed());
    assert_eq!((result.passed_count(), result.failed_count()), (1, 1));
}

#[test]
#[serial]
fn repeated_identical_successful_contract_executes_once() {
    let fixture = Fixture::new("printf 'constant output'");
    let mut stage = extended_stage("sh scripts/probe.sh", &["constant output"], &[], false, 0);
    stage.sandbox.command_confinement = Some(CommandConfinement::Confined);
    let config = fixture.config(Duration::from_secs(2));

    let first = run(&fixture, &stage, &config);
    let second = run(&fixture, &stage, &config);
    let third = run(&fixture, &stage, &config);

    assert!(first.all_passed() && second.all_passed() && third.all_passed());
    assert_eq!(
        (
            first.results()[0].cached,
            second.results()[0].cached,
            third.results()[0].cached,
            fixture.executions(),
        ),
        (false, true, true, 1)
    );
    assert!(second.results()[0].stdout.contains("constant output"));
}

#[test]
#[serial]
fn successful_contract_source_mutation_misses_and_executes() {
    let fixture = Fixture::new("printf 'constant output'");
    let mut stage = extended_stage("sh scripts/probe.sh", &["constant output"], &[], false, 0);
    stage.sandbox.command_confinement = Some(CommandConfinement::Confined);
    let config = fixture.config(Duration::from_secs(2));

    let first = run(&fixture, &stage, &config);
    let hit = run(&fixture, &stage, &config);
    fixture.mutate_script();
    let changed = run(&fixture, &stage, &config);

    assert!(first.all_passed() && hit.all_passed() && changed.all_passed());
    assert!(!first.results()[0].cached && hit.results()[0].cached);
    assert!(!changed.results()[0].cached);
    assert_eq!(fixture.executions(), 2);
}

#[test]
#[serial]
fn forbidden_output_failure_executes_and_fails_twice() {
    let fixture = Fixture::new(":");
    let mut stage = extended_stage("printf forbidden", &[], &["forbidden"], false, 0);
    stage.setup.push("sh scripts/probe.sh".to_string());
    let config = fixture.config(Duration::from_secs(2));

    let first = run(&fixture, &stage, &config);
    let second = run(&fixture, &stage, &config);

    assert!(!first.all_passed() && !second.all_passed());
    assert!(!first.results()[0].cached && !second.results()[0].cached);
    assert!(first.results()[0].stdout.contains("forbidden"));
    assert!(second.failures()[0].contains("forbidden pattern"));
    assert_eq!(fixture.executions(), 2);
}

#[test]
#[serial]
fn nonempty_stderr_failure_is_never_cached() {
    let fixture = Fixture::new("printf 'diagnostic' >&2");
    let stage = extended_stage("sh scripts/probe.sh", &[], &[], true, 0);
    let config = fixture.config(Duration::from_secs(2));

    let first = run(&fixture, &stage, &config);
    let second = run(&fixture, &stage, &config);

    assert!(!first.all_passed() && !second.all_passed());
    assert!(second.failures()[0].contains("stderr was not empty"));
    assert_eq!(fixture.executions(), 2);
}

#[test]
#[serial]
fn expected_nonzero_pass_hits_and_preserves_exit_then_mutation_misses() {
    let fixture = Fixture::new("exit 7");
    let stage = extended_stage("sh scripts/probe.sh", &[], &[], false, 7);
    let config = fixture.config(Duration::from_secs(2));

    let first = run(&fixture, &stage, &config);
    let second = run(&fixture, &stage, &config);
    fixture.mutate_script();
    let third = run(&fixture, &stage, &config);

    assert!(first.all_passed() && second.all_passed() && third.all_passed());
    assert_eq!(second.results()[0].exit_code, Some(7));
    assert!(second.results()[0].cached && !third.results()[0].cached);
    assert_eq!(fixture.executions(), 2);
}

#[test]
#[serial]
fn changed_pattern_and_simple_to_extended_contracts_miss() {
    let fixture = Fixture::new("printf 'alpha beta'");
    let alpha = extended_stage("sh scripts/probe.sh", &["alpha"], &[], false, 0);
    let beta = extended_stage("sh scripts/probe.sh", &["beta"], &[], false, 0);
    let simple = simple_stage("sh scripts/probe.sh");
    let config = fixture.config(Duration::from_secs(2));

    let first = run(&fixture, &simple, &config);
    let second = run(&fixture, &alpha, &config);
    let third = run(&fixture, &beta, &config);

    assert!(first.all_passed() && second.all_passed() && third.all_passed());
    assert!(first.results().iter().all(|result| !result.cached));
    assert!(second.results().iter().all(|result| !result.cached));
    assert!(third.results().iter().all(|result| !result.cached));
    assert_eq!(fixture.executions(), 3);
}

#[test]
#[serial]
fn identical_simple_contract_hits_but_timeout_mutation_misses() {
    let fixture = Fixture::new(":");
    let stage = simple_stage("sh scripts/probe.sh");
    let two_seconds = fixture.config(Duration::from_secs(2));
    let three_seconds = fixture.config(Duration::from_secs(3));

    let first = run(&fixture, &stage, &two_seconds);
    let second = run(&fixture, &stage, &two_seconds);
    let third = run(&fixture, &stage, &three_seconds);

    assert!(first.all_passed() && second.all_passed() && third.all_passed());
    assert_eq!(
        (
            first.results()[0].cached,
            second.results()[0].cached,
            third.results()[0].cached,
            fixture.executions(),
        ),
        (false, true, false, 2)
    );
    assert_eq!(second.total_duration(), second.results()[0].duration);
}

#[test]
#[serial]
fn confinement_change_misses_and_inherit_always_bypasses() {
    let fixture = Fixture::new(":");
    let mut stage = simple_stage("sh scripts/probe.sh");
    let config = fixture.config(Duration::from_secs(2));
    let first = run(&fixture, &stage, &config);
    let hit = run(&fixture, &stage, &config);
    stage.sandbox.command_confinement = Some(CommandConfinement::Inherit);
    let inherited_one = run(&fixture, &stage, &config);
    let inherited_two = run(&fixture, &stage, &config);

    assert!(first.all_passed() && hit.all_passed());
    assert!(inherited_one.all_passed() && inherited_two.all_passed());
    assert!(hit.results()[0].cached);
    assert!(!inherited_one.results()[0].cached && !inherited_two.results()[0].cached);
    assert_eq!(fixture.executions(), 3);
}

struct EnvGuard {
    key: &'static str,
    original: Option<String>,
}

impl EnvGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let original = std::env::var(key).ok();
        std::env::set_var(key, value);
        Self { key, original }
    }

    fn change(&self, value: &str) {
        std::env::set_var(self.key, value);
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.original {
            Some(value) => std::env::set_var(self.key, value),
            None => std::env::remove_var(self.key),
        }
    }
}

#[test]
#[serial]
fn changed_allowlisted_environment_misses() {
    let guard = EnvGuard::set("LANG", "C");
    let fixture = Fixture::new(":");
    let stage = simple_stage("sh scripts/probe.sh");
    let config = fixture.config(Duration::from_secs(2));
    let first = run(&fixture, &stage, &config);
    let hit = run(&fixture, &stage, &config);
    guard.change("POSIX");
    let third = run(&fixture, &stage, &config);

    assert!(first.all_passed() && hit.all_passed() && third.all_passed());
    assert!(!first.results()[0].cached);
    assert!(hit.results()[0].cached);
    assert!(!third.results()[0].cached);
    assert_eq!(fixture.executions(), 2);
}

#[test]
fn bypass_policy_executes_every_time() {
    let fixture = Fixture::new(":");
    let stage = simple_stage("sh scripts/probe.sh");
    let config = CriteriaConfig::default()
        .with_cache_dir(fixture.cache.path())
        .with_cache_policy(CachePolicy::Bypass);

    assert!(run(&fixture, &stage, &config).all_passed());
    assert!(run(&fixture, &stage, &config).all_passed());
    assert_eq!(fixture.executions(), 2);
}
