//! Several stage journals read as one set of entries: what `loom memory
//! pending` lists and the integration-verify signal renders.

use std::collections::HashSet;
use std::path::Path;

use anyhow::Result;

use super::types::{MemoryEntry, MemoryJournal};

/// Entries paired with the stage whose journal holds each one.
pub type StagedEntries = Vec<(String, MemoryEntry)>;

/// Every entry of the `stages` journals under `work_dir`, each journal read
/// once through `read`, paired with the stage whose journal holds it. Stages
/// are read in sorted order; an id already seen in an earlier journal is
/// skipped. Returns the entries and the sorted, deduplicated stages.
pub fn read_staged_entries(
    work_dir: &Path,
    mut stages: Vec<String>,
    read: impl Fn(&Path, &str) -> Result<MemoryJournal>,
) -> Result<(StagedEntries, Vec<String>)> {
    stages.sort();
    stages.dedup();
    let mut seen_ids = HashSet::new();
    let mut entries = Vec::new();
    for stage in &stages {
        for entry in read(work_dir, stage)?.entries {
            if seen_ids.insert(entry.id.clone()) {
                entries.push((stage.clone(), entry));
            }
        }
    }
    Ok((entries, stages))
}

/// The ids of the entries that a receipt among `entries` settles.
pub fn settled_ids<'a>(entries: impl IntoIterator<Item = &'a MemoryEntry>) -> HashSet<String> {
    entries
        .into_iter()
        .filter_map(|entry| entry.receipt.as_ref())
        .map(|receipt| receipt.event_id.clone())
        .collect()
}
