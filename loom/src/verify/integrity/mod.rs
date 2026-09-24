//! Test integrity (DESIGN D13): what a stage's changes did to the tests it
//! found at its base, the merge base the review gate uses (D12). `count`
//! compares each language's declaration and assertion totals, `edits` finds
//! assertion lines a base test file lost and ratchet files that changed, and
//! `gate` refuses completion until `reviews/<stage>/integrity.json` accepts
//! every event and none got worse.

mod count;
mod edits;
mod gate;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path};

use crate::testrun::languages;
use crate::verify::review::fingerprint::{self, ChangeFingerprint};
use crate::verify::review::store::{self, RECORD_VERSION};

pub use gate::{check, describe, shortfall};

const ACCEPTED_FILE: &str = "integrity.json";
/// Directory names whose files are tests whatever their language.
const TEST_DIRS: [&str; 4] = ["test", "tests", "__tests__", "spec"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntegrityEvent {
    /// `TI-decl-<lang>`, `TI-assert-<lang>`, `TI-edit-<path>` or `TI-ratchet-<path>`.
    pub id: String,
    pub kind: EventKind,
    pub language: Option<String>,
    pub path: Option<String>,
    /// Count events: the language's total at base.
    pub base: Option<u64>,
    /// Count events: the language's total now.
    pub current: Option<u64>,
    /// Edit and ratchet events: the file's sha256 hex now, `None` once deleted.
    pub current_sha256: Option<String>,
    /// Edit events: the lost assertion lines, trimmed.
    pub detail: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    DeclTotal,
    AssertTotal,
    AssertionEdit,
    Ratchet,
}

/// `reviews/<stage>/integrity.json`: the events integrity disputes accepted.
/// An absent file accepts nothing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Accepted {
    pub version: u32,
    pub accepted: Vec<AcceptedEvent>,
}

impl Default for Accepted {
    fn default() -> Self {
        Self {
            version: RECORD_VERSION,
            accepted: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptedEvent {
    pub event: String,
    pub kind: EventKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<u64>,
    /// Count events: the lowest total the dispute accepted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accepted_current: Option<u64>,
    /// Edit and ratchet events: the file content the dispute accepted; `None`
    /// accepts the file's deletion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accepted_sha256: Option<String>,
    pub dispute: u32,
}

/// What [`scan`] found: the integrity events, and the changed test files no
/// language profile covers, which go uncounted.
#[derive(Debug, Clone, PartialEq)]
pub struct Scan {
    pub events: Vec<IntegrityEvent>,
    pub unprofiled: Vec<String>,
}

/// The worktree's integrity events against `git merge-base HEAD <target_branch>`.
pub fn current_events(
    worktree: &Path,
    target_branch: &str,
    ratchet_files: &[String],
) -> Result<Vec<IntegrityEvent>> {
    Ok(scan(worktree, target_branch, ratchet_files)?.events)
}

/// [`current_events`], plus the unprofiled test files a caller notes.
pub fn scan(worktree: &Path, target_branch: &str, ratchet_files: &[String]) -> Result<Scan> {
    let changes = fingerprint::compute(worktree, target_branch)
        .context("failed to list the worktree's changes for the test-integrity check")?;
    let base_files = count::base_test_files(worktree, &changes.base)?;
    let mut events = count::total_events(worktree, &changes, &base_files)?;
    events.extend(edits::assertion_edits(worktree, &changes, &base_files)?);
    events.extend(edits::ratchet_events(&changes, ratchet_files));
    let unprofiled = changes
        .files
        .keys()
        .filter(|path| languages::for_path(path).is_none() && looks_like_test(path))
        .cloned()
        .collect();
    Ok(Scan { events, unprofiled })
}

/// `reviews/<stage>/integrity.json` in `work_dir`.
pub fn load_accepted(work_dir: &Path, stage_id: &str) -> Result<Accepted> {
    let accepted: Accepted =
        store::load_optional(work_dir, stage_id, ACCEPTED_FILE)?.unwrap_or_default();
    store::check_version(
        accepted.version,
        &format!("{ACCEPTED_FILE} of stage '{stage_id}'"),
    )?;
    Ok(accepted)
}

/// The sha256 hex of a changed path as the fingerprint hashed it; `None` for a
/// deleted path.
fn current_sha256(changes: &ChangeFingerprint, path: &str) -> Option<String> {
    changes
        .files
        .get(path)
        .filter(|digest| digest.as_str() != "deleted")
        .cloned()
}

/// A file in a test directory, or one whose name has a `test` or `spec` word.
fn looks_like_test(path: &str) -> bool {
    let path = Path::new(path);
    let in_test_dir = path.parent().is_some_and(|dir| {
        dir.components().any(|component| {
            matches!(component, Component::Normal(name)
                if name.to_str().is_some_and(|name| TEST_DIRS.contains(&name)))
        })
    });
    let named_test = path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.to_ascii_lowercase()
                .split(['_', '-', '.'])
                .any(|word| matches!(word, "test" | "tests" | "spec"))
        });
    in_test_dir || named_test
}

#[cfg(test)]
mod tests;
