//! Bounded discovery of Claude parent directories.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::io;

use anyhow::{ensure, Result};

use super::super::ledger::StartedAgentTypeIndex;
use super::super::resolve::project_slug;
use super::identity::{validate_parent_uuid, IdentityScope};

const MAX_PARENT_DIRS: usize = 512;

pub(super) fn claude_parent_candidates(
    scope: &IdentityScope,
    agent_id: &str,
    starts: &StartedAgentTypeIndex,
) -> Result<BTreeSet<String>> {
    let project = scope
        .projects_root
        .join(project_slug(&scope.canonical_worktree));
    let entries = match fs::read_dir(project) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(BTreeSet::new()),
        Err(error) => return Err(error.into()),
    };
    let entries = entries.map(|entry| {
        let entry = entry?;
        let name = entry.file_name();
        let is_dir = if name.to_str().is_some() {
            entry.file_type()?.is_dir()
        } else {
            false
        };
        Ok((name, is_dir))
    });
    candidates_from_entries(scope, agent_id, starts, entries)
}

pub(super) fn candidates_from_entries(
    scope: &IdentityScope,
    agent_id: &str,
    starts: &StartedAgentTypeIndex,
    entries: impl Iterator<Item = io::Result<(OsString, bool)>>,
) -> Result<BTreeSet<String>> {
    let mut parents = BTreeSet::new();
    for (index, entry) in entries.take(MAX_PARENT_DIRS + 1).enumerate() {
        ensure!(
            index < MAX_PARENT_DIRS,
            "Claude parent directory scan exceeds {MAX_PARENT_DIRS} entries; specify --session"
        );
        let (name, is_dir) = entry?;
        let Some(parent) = name.to_str() else {
            continue;
        };
        if is_dir
            && validate_parent_uuid(parent).is_ok()
            && starts
                .resolve_exact(&scope.stage_id, parent, &scope.loom_session_id, agent_id)
                .is_some()
        {
            parents.insert(parent.to_owned());
        }
    }
    Ok(parents)
}
