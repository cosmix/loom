//! A run granted the memory journals neither answers from nor stores in the
//! pass cache: the journals are no input the fingerprint holds.

use std::path::Path;

use tempfile::TempDir;

use crate::models::stage::{AcceptanceCriterion, Stage};
use crate::process::sandbox_probe::{confined_commands_runnable, skip_unless};
use crate::test_git::{git, TestRepo};
use crate::verify::criteria::config::CriteriaConfig;
use crate::verify::criteria::runner::run_acceptance_with_config;
use crate::verify::criteria::{CachePolicy, MemoryJournals};

fn stored_files(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .map(|entry| entry.unwrap().path())
                .map(|path| {
                    if path.is_dir() {
                        stored_files(&path)
                    } else {
                        1
                    }
                })
                .sum()
        })
        .unwrap_or(0)
}

/// Two runs of one passing criterion; whether each answered from the cache,
/// and how many files the cache holds after them.
fn run_twice(journals: Option<MemoryJournals>) -> ([bool; 2], usize) {
    let repo = TestRepo::builder().no_seed().create();
    std::fs::write(repo.path().join("source.txt"), "source\n").unwrap();
    git(repo.path(), &["add", "source.txt"]);
    git(repo.path(), &["commit", "-q", "-m", "fixture"]);
    let cache = TempDir::new().unwrap();
    let mut stage = Stage::new("distill".to_string(), None);
    stage.add_acceptance_criterion(AcceptanceCriterion::Simple("true".to_string()));
    let config = CriteriaConfig::default()
        .with_cache_dir(cache.path())
        .with_cache_policy(CachePolicy::Use)
        .with_memory_journals(journals);
    let cached = [(); 2].map(|()| {
        let result = run_acceptance_with_config(&stage, Some(repo.path()), &config).unwrap();
        assert!(result.all_passed());
        result.results()[0].cached
    });
    (cached, stored_files(cache.path()))
}

#[test]
fn a_run_granted_the_journals_skips_the_pass_cache() {
    if skip_unless(
        confined_commands_runnable(),
        "a_run_granted_the_journals_skips_the_pass_cache",
        "loom's confinement cannot start here",
    ) {
        return;
    }
    // The same fixture without the grant caches its pass, so the skip below
    // is the grant's doing.
    let (cached, stored) = run_twice(None);
    assert_eq!(cached, [false, true]);
    assert!(stored > 0);

    let journals = TempDir::new().unwrap();
    let dir = journals.path().canonicalize().unwrap();
    let grant = MemoryJournals {
        dir: dir.clone(),
        linked: dir,
    };
    assert_eq!(run_twice(Some(grant)), ([false, false], 0));
}
