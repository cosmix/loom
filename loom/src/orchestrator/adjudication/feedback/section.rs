//! Appending one notice section to the round's feedback file.
//!
//! A verdict is applied before its receipt is persisted, so a failed or interrupted application
//! is applied again on the next tick: appending a section the file already holds is a no-op.

use anyhow::{Context, Result};
use std::io::Write;
use std::path::Path;

use super::{feedback_path, open_feedback};
use crate::fs::safe_read::{read_to_string_bounded, OverLimit};

const MAX_FEEDBACK_BYTES: usize = 64 * 1024;
pub(super) const SECTION_SEPARATOR: &str = "\n---\n\n";
const DROPPED_NOTICES: &str =
    "Earlier notices of this round were dropped: the file passed 64 KiB.\n";

/// Append `body` as a new section, unless the file already holds it. A file that would pass
/// [`MAX_FEEDBACK_BYTES`] is truncated first, and the drop notice marks only a file that had
/// content to drop. Callers hold the `disputes/<stage>/.lock` flock, which serialises appends.
pub(super) fn append_section(work_dir: &Path, stage_id: &str, body: &str) -> Result<()> {
    let path = feedback_path(work_dir, stage_id);
    let mut file = open_feedback(&path)?;
    let len = file
        .metadata()
        .with_context(|| format!("Failed to stat {}", path.display()))?
        .len();
    if len > 0 && already_holds(&path, body)? {
        return Ok(());
    }
    let separator = if len == 0 { "" } else { SECTION_SEPARATOR };
    let size = len.saturating_add((separator.len() + body.len()) as u64);
    let prefix = if size > MAX_FEEDBACK_BYTES as u64 {
        file.set_len(0)
            .with_context(|| format!("Failed to truncate {}", path.display()))?;
        if len > 0 {
            DROPPED_NOTICES
        } else {
            ""
        }
    } else {
        separator
    };
    file.write_all(prefix.as_bytes())
        .and_then(|()| file.write_all(body.as_bytes()))
        .with_context(|| format!("Failed to append to {}", path.display()))
}

/// Whether the file at `path` holds `body` as a whole section. The read anchors at the parent
/// directory and follows no symlink. A file over the bound is about to be truncated, so it
/// holds nothing worth keeping.
fn already_holds(path: &Path, body: &str) -> Result<bool> {
    let dir = path.parent().context("feedback path has no parent")?;
    let name = path.file_name().context("feedback path has no filename")?;
    match read_to_string_bounded(dir, Path::new(name), MAX_FEEDBACK_BYTES) {
        Ok(existing) => Ok(holds_section(&existing, body)),
        Err(error) if error.downcast_ref::<OverLimit>().is_some() => Ok(false),
        Err(error) => Err(error).with_context(|| format!("Failed to read {}", path.display())),
    }
}

/// `body` is the whole file, or a section between [`SECTION_SEPARATOR`]s or after the drop
/// notice.
fn holds_section(existing: &str, body: &str) -> bool {
    let existing = existing.strip_prefix(DROPPED_NOTICES).unwrap_or(existing);
    existing == body
        || existing.starts_with(&format!("{body}{SECTION_SEPARATOR}"))
        || existing.ends_with(&format!("{SECTION_SEPARATOR}{body}"))
        || existing.contains(&format!("{SECTION_SEPARATOR}{body}{SECTION_SEPARATOR}"))
}

#[cfg(test)]
#[path = "section_tests.rs"]
mod tests;
