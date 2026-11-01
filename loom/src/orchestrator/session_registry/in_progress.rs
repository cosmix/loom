//! Validated persisted session inventory for ownership decisions.

use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::models::session::{Session, SessionStatus};
use crate::parser::frontmatter::parse_from_markdown;

const SESSION_LIMIT: usize = 1024 * 1024;

/// Valid records and failures discovered during one directory pass. Callers
/// deciding whether to spawn or retire a writer must require `strict_records`.
pub(super) struct SessionInventory {
    records: Vec<Session>,
    diagnostics: Vec<String>,
}

impl SessionInventory {
    pub(super) fn strict_records(self) -> Result<Vec<Session>> {
        if !self.diagnostics.is_empty() {
            bail!(
                "session inventory is uncertain: {}",
                self.diagnostics.join("; ")
            );
        }
        Ok(self.records)
    }

    #[cfg(test)]
    pub(super) fn records(self) -> Vec<Session> {
        self.records
    }
}

/// Read each session file once, retaining every failure. A never-created
/// sessions directory is empty; an existing directory that cannot be read is
/// an error. Records are bounded and opened without following symlinks.
pub(super) fn session_inventory(work_dir: &Path) -> Result<SessionInventory> {
    // Workspace discovery may intentionally return the `.loom/work` symlink.
    // Resolve that trusted root once, then keep enumeration and no-follow
    // reads beneath the same concrete directory for this pass.
    let root = work_dir
        .canonicalize()
        .with_context(|| format!("resolving session state root {}", work_dir.display()))?;
    let directory = root.join("sessions");
    let entries = match open_session_directory(&directory)? {
        Some(entries) => entries,
        None => {
            return Ok(SessionInventory {
                records: Vec::new(),
                diagnostics: Vec::new(),
            })
        }
    };
    let mut inventory = SessionInventory {
        records: Vec::new(),
        diagnostics: Vec::new(),
    };
    for entry in entries {
        let path = match entry {
            Ok(entry) => entry.path(),
            Err(error) => {
                inventory
                    .diagnostics
                    .push(format!("reading {}: {error}", directory.display()));
                continue;
            }
        };
        if path.extension().is_none_or(|ext| ext != "md") {
            continue;
        }
        match read_validated_session(&root, &path) {
            Ok(session) => inventory.records.push(session),
            Err(error) => inventory
                .diagnostics
                .push(format!("{}: {error:#}", path.display())),
        }
    }
    Ok(inventory)
}

fn open_session_directory(directory: &Path) -> Result<Option<std::fs::ReadDir>> {
    let metadata = match std::fs::symlink_metadata(directory) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| format!("inspecting {}", directory.display()))
        }
    };
    anyhow::ensure!(
        metadata.is_dir(),
        "session directory {} is not a directory",
        directory.display()
    );
    std::fs::read_dir(directory)
        .with_context(|| format!("reading {}", directory.display()))
        .map(Some)
}

fn read_validated_session(work_dir: &Path, path: &Path) -> Result<Session> {
    let relative = path.strip_prefix(work_dir)?;
    let content = crate::fs::safe_read::read_to_string_bounded(work_dir, relative, SESSION_LIMIT)?;
    let session = parse_from_markdown::<Session>(&content, "Session")?;
    crate::fs::session_files::validate_session_file_id(&session.id)?;
    let file_id = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .context("session filename is not valid UTF-8")?;
    anyhow::ensure!(
        file_id == session.id,
        "session filename '{file_id}' does not match record id '{}'",
        session.id
    );
    Ok(session)
}

/// Every persisted `Running` or `Spawning` record assigned to `stage_id`,
/// whether or not its process is still alive. Uncertainty blocks takedown.
pub fn in_progress_sessions_for_stage(work_dir: &Path, stage_id: &str) -> Result<Vec<Session>> {
    Ok(session_inventory(work_dir)?
        .strict_records()?
        .into_iter()
        .filter(|session| {
            session.stage_id.as_deref() == Some(stage_id)
                && matches!(
                    session.status,
                    SessionStatus::Running | SessionStatus::Spawning
                )
        })
        .collect())
}
