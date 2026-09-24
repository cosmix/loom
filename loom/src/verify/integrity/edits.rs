//! `TI-edit-<path>`: assertion lines a base test file lost, removed or changed
//! with no identical line added back in that file. `TI-ratchet-<path>`: a
//! ratchet file whose content differs from base.

use anyhow::Result;
use regex::Regex;
use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use super::count::{matchers, BaseTestFile};
use super::{current_sha256, EventKind, IntegrityEvent};
use crate::verify::contracts::changes::git;
use crate::verify::review::fingerprint::ChangeFingerprint;

/// One event per changed base test file whose diff from base loses an
/// assertion line.
pub(super) fn assertion_edits(
    worktree: &Path,
    changes: &ChangeFingerprint,
    base_files: &[BaseTestFile],
) -> Result<Vec<IntegrityEvent>> {
    let mut events = Vec::new();
    for file in base_files {
        if !changes.files.contains_key(&file.path) {
            continue;
        }
        let pathspec = format!(":(literal){}", file.path);
        let diff = git(
            worktree,
            &[
                "diff",
                "--no-color",
                "--no-ext-diff",
                "--no-textconv",
                "--text",
                "--unified=0",
                &changes.base,
                "--",
                &pathspec,
            ],
        )?;
        let assertion = &matchers(file.profile).assertion;
        let lost = lost_assertions(&String::from_utf8_lossy(&diff), assertion);
        if !lost.is_empty() {
            let language = Some(file.profile.name.to_string());
            events.push(file_event(
                EventKind::AssertionEdit,
                &file.path,
                language,
                changes,
                lost,
            ));
        }
    }
    Ok(events)
}

/// One event per ratchet file the worktree changed from base.
pub(super) fn ratchet_events(
    changes: &ChangeFingerprint,
    ratchet_files: &[String],
) -> Vec<IntegrityEvent> {
    ratchet_files
        .iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|path| changes.files.contains_key(path.as_str()))
        .map(|path| file_event(EventKind::Ratchet, path, None, changes, Vec::new()))
        .collect()
}

/// The trimmed text of every removed line matching `assertion`, less one
/// occurrence per identical added line: a moved line is added back, a changed
/// or deleted one is not.
fn lost_assertions(diff: &str, assertion: &Regex) -> Vec<String> {
    let mut removed = Vec::new();
    let mut added: HashMap<&str, usize> = HashMap::new();
    let mut in_hunk = false;
    for line in diff.lines() {
        if line.starts_with("diff ") {
            in_hunk = false;
        } else if line.starts_with("@@") {
            in_hunk = true;
        } else if !in_hunk {
            continue;
        } else if let Some(text) = line.strip_prefix('-') {
            if assertion.is_match(text) {
                removed.push(text.trim());
            }
        } else if let Some(text) = line.strip_prefix('+') {
            *added.entry(text.trim()).or_default() += 1;
        }
    }
    removed
        .into_iter()
        .filter(|text| match added.get_mut(text) {
            Some(count) if *count > 0 => {
                *count -= 1;
                false
            }
            _ => true,
        })
        .map(str::to_string)
        .collect()
}

fn file_event(
    kind: EventKind,
    path: &str,
    language: Option<String>,
    changes: &ChangeFingerprint,
    detail: Vec<String>,
) -> IntegrityEvent {
    let prefix = match kind {
        EventKind::Ratchet => "TI-ratchet",
        _ => "TI-edit",
    };
    IntegrityEvent {
        id: format!("{prefix}-{path}"),
        kind,
        language,
        path: Some(path.to_string()),
        base: None,
        current: None,
        current_sha256: current_sha256(changes, path),
        detail,
    }
}
