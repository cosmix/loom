//! Shared, read-only inventory for discovery and cleanup.
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::parser::markdown::MarkdownDocument;

pub struct SessionRecord {
    pub path: PathBuf,
    pub id: String,
    pub stage_id: Option<String>,
}

#[derive(Default)]
pub struct SessionEnumeration {
    pub records: Vec<SessionRecord>,
    pub diagnostics: Vec<String>,
}

pub fn enumerate_sessions(work_dir: &Path) -> Result<SessionEnumeration> {
    let directory = work_dir.join("sessions");
    let entries = match std::fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Default::default()),
        Err(error) => return Err(error).context("Failed to enumerate sessions"),
    };
    let mut result = SessionEnumeration::default();
    let mut paths = Vec::new();
    for entry in entries {
        match entry {
            Ok(entry) if entry.path().extension().and_then(|s| s.to_str()) == Some("md") => {
                paths.push(entry.path())
            }
            Ok(_) => {}
            Err(error) => result
                .diagnostics
                .push(format!("Failed to enumerate session: {error}")),
        }
    }
    paths.sort();
    let mut identities = HashSet::new();
    for path in paths {
        match read_record(work_dir, &path) {
            Ok(record) if identities.insert(record.id.clone()) => result.records.push(record),
            Ok(record) => result.diagnostics.push(format!(
                "Duplicate session identity '{}' in {}",
                record.id,
                path.display()
            )),
            Err(error) => result.diagnostics.push(format!(
                "Invalid session file '{}': {error:#}",
                path.display()
            )),
        }
    }
    Ok(result)
}

fn read_record(work_dir: &Path, path: &Path) -> Result<SessionRecord> {
    let relative = path.strip_prefix(work_dir)?;
    let content = crate::fs::safe_read::read_to_string_bounded(work_dir, relative, 1024 * 1024)?;
    let document = MarkdownDocument::parse(&content)?;
    let id = document
        .get_frontmatter("id")
        .context("Missing session id")?
        .clone();
    super::validate_session_file_id(&id)?;
    let stage_id = document.get_frontmatter("stage_id").cloned();
    if let Some(stage_id) = &stage_id {
        crate::validation::validate_id(stage_id)?;
    }
    Ok(SessionRecord {
        path: path.to_path_buf(),
        id,
        stage_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_duplicate_and_unreadable_records_share_diagnostics() {
        let temp = tempfile::tempdir().unwrap();
        let directory = temp.path().join("sessions");
        std::fs::create_dir(&directory).unwrap();
        let record = "---\nid: session-a\nstage_id: stage-a\n---\n";
        std::fs::write(directory.join("a.md"), record).unwrap();
        std::fs::write(directory.join("b.md"), record).unwrap();
        std::fs::write(directory.join("c.md"), "bad").unwrap();
        std::os::unix::fs::symlink("missing", directory.join("d.md")).unwrap();
        let inventory = enumerate_sessions(temp.path()).unwrap();
        assert_eq!(inventory.records.len(), 1);
        assert_eq!(inventory.diagnostics.len(), 3);
        assert_eq!(
            super::super::find_sessions_for_stage("stage-a", temp.path()).unwrap(),
            vec!["session-a"]
        );
        std::fs::remove_file(directory.join("a.md")).unwrap();
        assert!(read_record(temp.path(), &directory.join("a.md")).is_err());
    }
}
