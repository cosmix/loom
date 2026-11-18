//! Handoff file numbering and lookup utilities.

use anyhow::{Context, Result};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// Enumerate numbered handoffs in descending numeric order.
pub(super) fn numbered_handoffs(stage_id: &str, work_dir: &Path) -> Result<Vec<(u32, PathBuf)>> {
    let dir = work_dir.join("handoffs");
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("Failed to read handoffs directory: {}", dir.display()));
        }
    };
    let prefix = format!("{stage_id}-handoff-");
    let mut candidates = Vec::new();
    for entry in entries {
        let entry = entry.context("Failed to read handoff directory entry")?;
        let filename = entry.file_name();
        let filename = filename.to_string_lossy();
        let Some(number) = filename
            .strip_prefix(&prefix)
            .and_then(|rest| rest.strip_suffix(".md"))
            .filter(|number| !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit()))
            .and_then(|number| number.parse::<u32>().ok())
        else {
            continue;
        };
        candidates.push((number, entry.path()));
    }
    candidates.sort_unstable_by(|left, right| right.cmp(left));
    Ok(candidates)
}

/// Get the next sequential handoff number for a stage.
pub fn get_next_handoff_number(stage_id: &str, work_dir: &Path) -> Result<u32> {
    let max = numbered_handoffs(stage_id, work_dir)?
        .first()
        .map_or(0, |(number, _)| *number);
    max.checked_add(1).context("Handoff number space exhausted")
}

/// Find the most recent handoff file for a stage.
pub fn find_latest_handoff(stage_id: &str, work_dir: &Path) -> Result<Option<PathBuf>> {
    Ok(numbered_handoffs(stage_id, work_dir)?
        .into_iter()
        .next()
        .map(|(_, path)| path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbering_rejects_exhaustion_and_ignores_malformed_names() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("handoffs");
        fs::create_dir_all(&dir).unwrap();
        for name in [
            "stage-1-handoff-002.md",
            "stage-1-handoff-+42.md",
            "stage-1-handoff-4294967296.md",
        ] {
            fs::write(dir.join(name), "original").unwrap();
        }
        assert_eq!(get_next_handoff_number("stage-1", temp.path()).unwrap(), 3);
        fs::write(dir.join("stage-1-handoff-4294967295.md"), "maximum").unwrap();
        assert!(get_next_handoff_number("stage-1", temp.path()).is_err());
        assert!(find_latest_handoff("stage-1", temp.path())
            .unwrap()
            .unwrap()
            .ends_with("stage-1-handoff-4294967295.md"));
        assert_eq!(
            fs::read_to_string(dir.join("stage-1-handoff-002.md")).unwrap(),
            "original"
        );
    }
}
