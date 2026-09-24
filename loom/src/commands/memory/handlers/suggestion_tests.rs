//! A `suggestion` memory entry stays pending until an `implemented` (or any
//! other) receipt settles it; `implemented` needs a reason.

use super::super::record::record;
use super::super::resolve;
use super::super::tests::{init_git_repo, EnvGuard};
use super::groups::group_pending;
use super::pending_report;
use crate::fs::memory::{MemoryEntry, MemoryEntryType};
use serial_test::serial;
use std::env;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Enter a fresh repository outside any loom session; returns the guard that
/// restores the environment, the repository, and its work directory.
fn enter_repo() -> (EnvGuard, TempDir, PathBuf) {
    let guard = EnvGuard::new();
    let repo = init_git_repo();
    env::set_current_dir(repo.path()).unwrap();
    env::remove_var("LOOM_SESSION_ID");
    let work_dir = repo.path().join(".loom/work");
    (guard, repo, work_dir)
}

fn record_suggestion(stage: &str, text: &str) -> String {
    env::set_var("LOOM_STAGE_ID", stage);
    let entry = MemoryEntry::new(MemoryEntryType::Suggestion, text.to_string());
    let id = entry.id.clone();
    record(entry, None).unwrap();
    id
}

fn pending_ids(work_dir: &Path) -> Vec<String> {
    let report = pending_report(work_dir, None).unwrap();
    report
        .pending
        .into_iter()
        .map(|staged| staged.entry.id)
        .collect()
}

#[test]
#[serial]
fn suggestion_entries_are_pending_until_resolved() {
    let (_guard, _repo, work_dir) = enter_repo();
    let id = record_suggestion("suggestion-stage", "extract the retry loop");

    let grouped = group_pending(pending_report(&work_dir, None).unwrap());
    let grouped_ids: Vec<&str> = grouped
        .suggestions
        .iter()
        .map(|staged| staged.entry.id.as_str())
        .collect();
    assert_eq!(grouped_ids, vec![id.as_str()]);
    assert_eq!(grouped.pending_count(), 1);

    resolve(
        id,
        "implemented".to_string(),
        None,
        Some("x".to_string()),
        None,
    )
    .unwrap();

    assert!(pending_ids(&work_dir).is_empty());
}

#[test]
#[serial]
fn implemented_outcome_requires_reason() {
    let (_guard, _repo, work_dir) = enter_repo();
    let id = record_suggestion("implemented-stage", "rename the helper");

    let error = resolve(id.clone(), "implemented".to_string(), None, None, None).unwrap_err();

    assert!(error.to_string().contains("--reason"), "{error}");
    assert_eq!(pending_ids(&work_dir), vec![id]);
}
