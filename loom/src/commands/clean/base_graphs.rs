//! Base source-graph layer pruning for `loom clean`
//! (`doc/PROPOSAL-retrieval-precision.md` §A.14).
//!
//! Split out of `commands::clean` for the same reason `worktrees.rs` and
//! `sessions.rs` are: one concern per file, `commands::clean` itself stays
//! under the 400-line cap.
//!
//! `graph/base/*.json` accretes one file per published revision
//! (`context::graph_store::GraphStore::publish_base`) with no pruning until
//! now. This module runs the exact same retention
//! [`GraphStore::prune_base_graphs`] enforces at publish time, unconditionally,
//! on every `loom clean` invocation — this is disk hygiene, not a destructive
//! operation, so it does not need a flag to gate it the way `--worktrees` /
//! `--state` do.

use anyhow::Result;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::context::config::RetrievalConfig;
use crate::context::graph_store::GraphStore;
use crate::context::store::{ContextStore, CACHE_RELATIVE_DIR};

/// Prune stale base graph layers under `repo_root`'s context cache.
///
/// Returns `(files_removed, bytes_freed)`. Never destructive beyond what
/// [`GraphStore::prune_base_graphs`] already guarantees: the revision
/// `state.json` currently names as `semantic.revision`, plus the
/// `keep_base_graphs` most-recently-modified of the rest, always survive.
pub(super) fn prune_base_graphs(repo_root: &Path) -> Result<(usize, u64)> {
    let cache_root = repo_root.join(CACHE_RELATIVE_DIR);
    let store = ContextStore::with_root(&cache_root);
    let graph_store = GraphStore::new(&cache_root, &repo_root.join(".work"));
    let base_dir = graph_store.base_dir();

    let before = base_layer_sizes(&base_dir);
    if before.is_empty() {
        return Ok((0, 0));
    }

    let keep = RetrievalConfig::load(repo_root).keep_base_graphs;
    let protected_revision = store
        .load_state()
        .ok()
        .map(|state| state.semantic.revision)
        .filter(|revision| !revision.is_empty());
    let protected: Vec<&str> = protected_revision.as_deref().into_iter().collect();

    graph_store.prune_base_graphs(keep, &protected)?;

    let after = base_layer_sizes(&base_dir);
    let removed = before.len().saturating_sub(after.len());
    let freed_bytes: u64 = before
        .iter()
        .filter(|(path, _)| !after.contains_key(*path))
        .map(|(_, size)| *size)
        .sum();

    Ok((removed, freed_bytes))
}

/// `path -> byte size` for every `*.json` file directly in `dir`. Empty (not
/// an error) when `dir` does not exist yet — a project that has never
/// published a base graph has nothing to prune.
fn base_layer_sizes(dir: &Path) -> BTreeMap<PathBuf, u64> {
    let Ok(entries) = fs::read_dir(dir) else {
        return BTreeMap::new();
    };
    entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("json"))
        .filter_map(|entry| {
            let size = entry.metadata().ok()?.len();
            Some((entry.path(), size))
        })
        .collect()
}

/// Render a byte count as a human-friendly size, e.g. `1.2 MB`.
pub(super) fn human_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn human_bytes_formats_common_magnitudes() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1536), "1.5 KB");
        assert_eq!(human_bytes(5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn pruning_a_project_with_no_cache_removes_nothing() {
        let temp = TempDir::new().unwrap();
        let (removed, freed) = prune_base_graphs(temp.path()).unwrap();
        assert_eq!(removed, 0);
        assert_eq!(freed, 0);
    }
}
