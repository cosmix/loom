//! Base graph GC (`doc/PROPOSAL-retrieval-precision.md` §A.14):
//! [`GraphStore::prune_base_graphs`], plus the small helpers
//! [`GraphStore::publish_base`] needs to call it with the right `keep` count
//! and protected revision.
//!
//! Split out of `graph_store/mod.rs` to keep that file under the 400-line
//! cap — the same reason `refresh/semantic.rs` was split out of `refresh.rs`.

use anyhow::{Context, Result};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::GraphStore;
use crate::context::config::RetrievalConfig;
use crate::context::store::ContextStore;

impl GraphStore {
    /// The context cache root this store's base/overlay paths sit under
    /// (`<main project root>/.loom/cache/context-v1`) — two levels above
    /// [`Self::base_dir`] (`.../graph/base`), since [`super::GRAPH_RELATIVE_DIR`]
    /// and [`super::BASE_RELATIVE_DIR`] are each a single path component.
    /// This is exactly [`ContextStore::root`]'s value for the same project:
    /// it lets a `GraphStore` — which is handed only paths, never a
    /// `ContextStore` — read `state.json` for itself in
    /// [`Self::current_semantic_revision`] without widening its own public
    /// constructor to accept one.
    fn context_cache_root(&self) -> Option<PathBuf> {
        let base_dir = self.base_dir();
        let graph_root = base_dir.parent()?;
        let cache_root = graph_root.parent()?;
        Some(cache_root.to_path_buf())
    }

    /// The main project root this store's cache lives under, three levels
    /// above the cache root (`.loom`, `cache`, `context-v1` — see the
    /// `graph_root` field's doc comment on [`GraphStore`] for the full path
    /// this assumes). Used only to locate `.loom/config.toml` for
    /// [`RetrievalConfig::load`]; `None` degrades [`Self::keep_base_graphs`]
    /// to the compiled-in default, never a panic or a publish failure.
    fn derive_project_root(&self) -> Option<PathBuf> {
        let cache_root = self.context_cache_root()?;
        cache_root.ancestors().nth(3).map(Path::to_path_buf)
    }

    /// Best-effort read of the semantic revision `state.json` currently
    /// records, so [`Self::prune_after_publish`] can protect it from its own
    /// prune. `None` on any failure — missing file, unreadable, malformed,
    /// unresolvable project root, or simply never built — because a base
    /// publish must never fail or block on a read this module does not own:
    /// `state.json` belongs to [`ContextStore`] (see
    /// `doc/loom/knowledge/architecture/context-retrieval.md`'s "Derived vs
    /// Durable" section).
    fn current_semantic_revision(&self) -> Option<String> {
        let root = self.context_cache_root()?;
        let revision = ContextStore::with_root(root)
            .load_state()
            .ok()?
            .semantic
            .revision;
        (!revision.is_empty()).then_some(revision)
    }

    /// `keep_base_graphs` from `.loom/config.toml`, or the compiled-in
    /// default when the project root cannot be derived — `RetrievalConfig::load`
    /// itself never fails on a missing or unparseable file, so this never
    /// needs to either.
    fn keep_base_graphs(&self) -> usize {
        match self.derive_project_root() {
            Some(root) => RetrievalConfig::load(&root).keep_base_graphs,
            None => RetrievalConfig::default().keep_base_graphs,
        }
    }

    /// Called from [`Self::publish_base`] after every successful (newly
    /// written) publish. Resolves `keep` and the protected `state.json`
    /// revision itself, rather than taking them as parameters, so
    /// `publish_base`'s public signature never has to change for callers
    /// that only pass `revision` and `layer` — `commands/run/tests.rs` is
    /// one such caller outside this module's ownership.
    pub(super) fn prune_after_publish(&self, just_written_revision: &str) {
        let keep = self.keep_base_graphs();
        let current = self.current_semantic_revision();
        let mut protected: Vec<&str> = vec![just_written_revision];
        if let Some(current) = current.as_deref() {
            protected.push(current);
        }
        if let Err(error) = self.prune_base_graphs(keep, &protected) {
            tracing::debug!(%error, "base graph prune after publish failed");
        }
    }

    /// Prune published base layers, keeping the most useful few.
    ///
    /// Retains every stem named in `protected` — from
    /// [`Self::prune_after_publish`] this is the revision just written plus
    /// whatever `state.json` currently names, so a concurrent reader can
    /// never have its live base pulled out from under it — plus the `keep`
    /// most-recently-modified of whatever remains. Best-effort: an unlink
    /// failure is logged at `tracing::debug!` and never propagated — a base
    /// layer that could not be deleted costs disk, not correctness, and
    /// neither this module's own caller above nor `loom clean` (the other
    /// caller) may fail because of it.
    pub fn prune_base_graphs(&self, keep: usize, protected: &[&str]) -> Result<()> {
        let dir = self.base_dir();
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("Failed to list base graphs: {}", dir.display()));
            }
        };

        let mut candidates: Vec<(PathBuf, SystemTime)> = Vec::new();
        for entry in entries {
            let entry =
                entry.with_context(|| format!("Failed to read entry in {}", dir.display()))?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let is_protected = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .is_some_and(|stem| protected.contains(&stem));
            if is_protected {
                continue;
            }
            let modified = entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            candidates.push((path, modified));
        }

        // Newest first, so the `keep` survivors are exactly the first `keep`.
        candidates.sort_by_key(|b| std::cmp::Reverse(b.1));

        for (path, _) in candidates.into_iter().skip(keep) {
            if let Err(error) = fs::remove_file(&path) {
                tracing::debug!(path = %path.display(), %error, "failed to prune base graph layer");
            }
        }

        Ok(())
    }
}
