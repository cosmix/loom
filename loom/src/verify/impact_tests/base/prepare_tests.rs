//! Provisioning a base checkout: entry order, the comparison's own build cache
//! and the snapshot a malformed config refuses.

use super::*;
use crate::models::stage::Stage;
use crate::orchestrator::provision::write_provision_snapshot;
use crate::test_git::{git, TestRepo};
use crate::verify::impact_tests::base::provision_with;
use crate::verify::impact_tests::base::BaseCheckout;
use std::fs;
use tempfile::TempDir;

fn entry(command: &str) -> ProvisionEntry {
    ProvisionEntry {
        working_dir: ".".to_string(),
        command: command.to_string(),
    }
}

#[test]
fn entries_run_in_order_in_the_given_root_and_stop_at_the_first_failure() {
    let entries = [entry("first"), entry("second"), entry("third")];
    let root = Path::new("/the/checkout");
    let mut ran = Vec::new();

    let result = provision_with(&entries, root, |entry, dir| {
        ran.push((entry.command.clone(), dir.to_path_buf()));
        if entry.command == "second" {
            Err("second failed".to_string())
        } else {
            Ok(())
        }
    });

    assert_eq!(result, Err("second failed".to_string()));
    let expected = |command: &str| (command.to_string(), root.to_path_buf());
    assert_eq!(ran, [expected("first"), expected("second")]);
}

#[test]
fn every_entry_runs_when_none_fails() {
    let entries = [entry("first"), entry("second")];
    let mut ran = Vec::new();

    let result = provision_with(&entries, Path::new("/c"), |entry, _| {
        ran.push(entry.command.clone());
        Ok(())
    });

    assert_eq!(result, Ok(()));
    assert_eq!(ran, ["first", "second"]);
    assert_eq!(
        provision_with(&[], Path::new("/c"), |_, _| Err(String::new())),
        Ok(())
    );
}

/// A repository with one commit; returns it and the commit.
fn repo() -> (TempDir, String) {
    let temp = TempDir::new().unwrap();
    TestRepo::builder().no_seed().init_in(temp.path());
    fs::write(temp.path().join("a"), "a").unwrap();
    git(temp.path(), &["add", "."]);
    git(temp.path(), &["commit", "-q", "-m", "base"]);
    let commit = git(temp.path(), &["rev-parse", "HEAD"]);
    (temp, commit)
}

#[test]
fn the_comparison_has_a_build_cache_of_its_own_removed_with_its_checkout() {
    let (repo, commit) = repo();
    let first = BaseCheckout::create(repo.path(), &commit).unwrap();
    let second = BaseCheckout::create(repo.path(), &commit).unwrap();

    let cache = base_cache(first.root(), "s").unwrap();
    let other = base_cache(second.root(), "s").unwrap();
    let ensured = cache.ensure().unwrap();

    let scratch = first.root().parent().unwrap().canonicalize().unwrap();
    assert!(ensured.starts_with(&scratch), "{}", ensured.display());
    assert!(!ensured.starts_with(first.root().canonicalize().unwrap()));
    assert_ne!(cache.root(), other.root(), "one cache per comparison");
    assert!(
        !other.root().starts_with(first.root().parent().unwrap()),
        "a second comparison does not share the first's directory"
    );
    drop(first);
    assert!(!ensured.exists(), "the cache is removed with its checkout");
}

#[test]
fn a_malformed_provision_snapshot_is_a_configuration_error() {
    let work = TempDir::new().unwrap();
    let stage = Stage::default();
    fs::write(
        work.path().join("config.toml"),
        "[plan_provision]\nentries = \"x\"\n",
    )
    .unwrap();

    let refused = BaseCriteria::new(&stage, CriteriaConfig::default(), work.path());

    let message = format!(
        "{:#}",
        refused.err().expect("a malformed snapshot is refused")
    );
    assert!(message.contains("provision snapshot"), "{message}");
}

#[test]
fn the_snapshot_entries_are_the_ones_provisioned() {
    let work = TempDir::new().unwrap();
    let stage = Stage::default();
    let none = BaseCriteria::new(&stage, CriteriaConfig::default(), work.path()).unwrap();
    assert!(none.entries.is_empty(), "no snapshot, no entries");
    write_provision_snapshot(work.path(), &[entry("one"), entry("two")]).unwrap();

    let some = BaseCriteria::new(&stage, CriteriaConfig::default(), work.path()).unwrap();

    let commands: Vec<_> = some.entries.iter().map(|e| e.command.as_str()).collect();
    assert_eq!(commands, ["one", "two"]);
}

/// A criteria runner for a stage whose commands run at `confinement`, with a
/// pass cache configured, and a checkout directory of a scratch comparison.
fn runner_at(confinement: CommandConfinement) -> (TempDir, std::path::PathBuf, Stage) {
    let scratch = TempDir::new().unwrap();
    let root = scratch.path().join("checkout");
    fs::create_dir(&root).unwrap();
    let mut stage = Stage {
        id: "s".to_string(),
        ..Stage::default()
    };
    stage.sandbox.command_confinement = Some(confinement);
    (scratch, root, stage)
}

fn criteria_with_pass_cache<'a>(stage: &'a Stage, work: &Path) -> BaseCriteria<'a> {
    let config = CriteriaConfig {
        cache_dir: Some(work.join("pass-cache")),
        ..CriteriaConfig::default()
    };
    BaseCriteria::new(stage, config, work).unwrap()
}

#[test]
fn confined_runs_read_the_cache_provision_fills_and_no_pass_cache() {
    let (scratch, root, stage) = runner_at(CommandConfinement::Confined);
    let work = TempDir::new().unwrap();
    let criteria = criteria_with_pass_cache(&stage, work.path());

    let config = criteria.probe_config(&root);
    let mut provisioned = None;
    criteria
        .provision_through(&root, |_, _, cache| {
            provisioned = Some(cache.root().to_path_buf());
            Ok(())
        })
        .unwrap();

    let granted = config
        .stage_cache
        .expect("a confined base run is handed the comparison's build cache");
    assert_eq!(Some(granted.root().to_path_buf()), provisioned);
    assert!(granted.root().starts_with(scratch.path()));
    assert_eq!(config.cache_dir, None, "a scratch copy keeps no pass cache");
    assert_eq!(config.tree_root.as_deref(), Some(root.as_path()));
}

#[test]
fn inherited_runs_are_not_handed_a_build_cache() {
    let (_scratch, root, stage) = runner_at(CommandConfinement::Inherit);
    let work = TempDir::new().unwrap();
    let criteria = criteria_with_pass_cache(&stage, work.path());

    let config = criteria.probe_config(&root);

    assert!(config.stage_cache.is_none());
    assert_eq!(config.cache_dir, None);
}
