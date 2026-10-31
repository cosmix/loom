//! Layered persistence for the derived source graph.
//!
//! ## Why layers exist
//!
//! Parallel stages run in separate worktrees off one repository. If they shared
//! a mutable graph, a stage would see half of a sibling's edits — worse than
//! seeing none, because there is no way to tell which half. So the graph is
//! split in two:
//!
//! - a **base layer**, keyed by the source revision it was built from, written
//!   once by the host and thereafter immutable. `.loom/cache/context-v1/graph/base/`
//!   under the canonical main project root, shared by every worktree;
//! - a per-stage **overlay**, under `.loom/work/context/<plan>/<stage>/`, holding
//!   only the files that stage changed.
//!
//! A read is `overlay ∪ (base − overlay's files)`: an overlay entry shadows the
//! base entry for the same path wholesale, never merges with it. Partial merges
//! are what produce a graph that describes no revision that ever existed.
//!
//! Nothing here builds a graph — `crate::context::refresh` does that — and
//! nothing here decides *when* to write one; that is
//! `crate::context::refresh::source_graph::reconcile_source_graph`'s job. This
//! module owns only the layout, the layering rule, and canonical serialization.

use anyhow::{Context, Result};
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::context::source_graph::FileCoverage;
use crate::context::store::canonical_json;
use crate::context::view::ResolvedView;

/// Graph directory, relative to the context cache root.
pub const GRAPH_RELATIVE_DIR: &str = "graph";
/// Immutable per-revision base layers, relative to [`GRAPH_RELATIVE_DIR`].
pub const BASE_RELATIVE_DIR: &str = "base";
/// Resolved views of base revisions, relative to [`GRAPH_RELATIVE_DIR`].
pub(crate) const VIEW_RELATIVE_DIR: &str = "view";
/// Overlay root inside `.loom/work/`.
pub const OVERLAY_RELATIVE_DIR: &str = "context";
/// File name of a persisted layer.
pub const LAYER_FILE: &str = "graph.json";

/// Resolves layer paths and reads/writes layers. Holds no graph state itself.
#[derive(Debug, Clone)]
pub struct GraphStore {
    /// `<main project root>/.loom/cache/context-v1/graph`.
    graph_root: PathBuf,
    /// `<.loom/work>/context`.
    overlay_root: PathBuf,
    /// Layers a denied disk write fell back to; see `fallback`.
    memory_fallback: RefCell<HashMap<PathBuf, GraphLayer>>,
    /// Views a denied disk write fell back to; see `view::store`.
    pub(crate) view_fallback: RefCell<HashMap<PathBuf, ResolvedView>>,
    /// Views this process materialized, keyed by view path; the first
    /// `GraphStore::view` call for one takes it. See `view::store`.
    pub(crate) view_cache: RefCell<HashMap<PathBuf, ResolvedView>>,
}

/// One overlay generation and the only base revision compatible with it.
pub(crate) struct GraphRead<'a> {
    pub revision: String,
    pub address: Option<(&'a str, &'a str)>,
    pub overlay: Option<GraphLayer>,
}

impl GraphStore {
    /// Build from the two roots directly. `context_cache_root` is
    /// [`crate::context::store::ContextStore::root`]; `work_root` is
    /// [`crate::fs::work_dir::WorkDir::root`].
    pub fn new(context_cache_root: &Path, work_root: &Path) -> Self {
        GraphStore {
            graph_root: context_cache_root.join(GRAPH_RELATIVE_DIR),
            overlay_root: work_root.join(OVERLAY_RELATIVE_DIR),
            memory_fallback: RefCell::new(HashMap::new()),
            view_fallback: RefCell::new(HashMap::new()),
            view_cache: RefCell::new(HashMap::new()),
        }
    }

    /// Directory holding immutable per-revision base layers.
    pub fn base_dir(&self) -> PathBuf {
        self.graph_root.join(BASE_RELATIVE_DIR)
    }

    /// Path of the base layer for `revision`.
    ///
    /// One file per revision, so a stage that started against an older base
    /// keeps reading a consistent snapshot after the host publishes a newer one.
    pub fn base_path(&self, revision: &str) -> PathBuf {
        self.base_dir().join(format!("{revision}.json"))
    }

    /// Directory of a stage's overlay: `.loom/work/context/<plan>/<stage>/`.
    pub fn overlay_dir(&self, plan: &str, stage: &str) -> PathBuf {
        self.overlay_root.join(plan).join(stage)
    }

    /// Path of a stage's overlay layer.
    pub fn overlay_path(&self, plan: &str, stage: &str) -> PathBuf {
        self.overlay_dir(plan, stage).join(LAYER_FILE)
    }

    /// Read the base layer for `revision`, or `None` when it was never built or
    /// its file does not parse. The schema is not checked here: callers that
    /// reuse the layer test `GraphLayer::has_current_schema`.
    pub fn load_base(&self, revision: &str) -> Result<Option<GraphLayer>> {
        self.read_layer_or_memory(&self.base_path(revision))
    }

    pub(crate) fn has_base_layer(&self, revision: &str) -> bool {
        let path = self.base_path(revision);
        !revision.is_empty()
            && (path.is_file() || self.memory_fallback.borrow().contains_key(&path))
    }

    /// Read the most recently written base layer, if any.
    pub(crate) fn load_newest_base(&self) -> Result<Option<GraphLayer>> {
        let dir = self.base_dir();
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("Failed to list base graphs: {}", dir.display()));
            }
        };
        let mut candidates: Vec<(SystemTime, PathBuf)> = Vec::new();
        for entry in entries {
            let entry =
                entry.with_context(|| format!("Failed to read entry in {}", dir.display()))?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let modified = entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            candidates.push((modified, path));
        }
        // Newest first; ties break on the larger path.
        candidates.sort();
        candidates.reverse();
        for (_, path) in candidates {
            // An unparseable or stale-schema base is skipped, never served: it
            // is a reuse source only when it describes the current schema.
            if let Some(layer) = read_layer(&path)?.filter(GraphLayer::has_current_schema) {
                return Ok(Some(layer));
            }
        }
        Ok(None)
    }

    /// Publish a base layer for `revision`.
    ///
    /// A published base layer is immutable: if one already exists for this
    /// revision, this is a no-op returning `false`, because two builds of the
    /// same revision must agree and rewriting would invalidate every overlay
    /// cut from it — and, deliberately, that early-return path never prunes:
    /// a rapid sequence of `publish_base` calls for an already-published
    /// revision must not each pay a GC pass for nothing.
    ///
    /// A NEWLY written base, on the other hand, triggers
    /// `Self::prune_after_publish` (`graph_store/prune.rs`) so publishing
    /// is the one place base-layer retention is enforced (see
    /// `doc/PROPOSAL-retrieval-precision.md` §A.14) — `graph/base/`
    /// otherwise accretes one file per published commit forever.
    ///
    /// A base that is stale or does not parse is not a published base: it is
    /// overwritten through [`Self::replace_base`] instead.
    pub fn publish_base(&self, revision: &str, layer: &GraphLayer) -> Result<bool> {
        let path = self.base_path(revision);
        if path.exists() {
            return Ok(false);
        }
        if self.write_or_fall_back(&path, layer)? {
            self.prune_after_publish(revision);
        }
        Ok(true)
    }

    /// Overwrite the base layer for `revision`, for a caller that found the
    /// one on disk stale or unparseable.
    ///
    /// The write is a temp-file-and-rename over the old file, never a delete
    /// followed by a write: a reader sees the old layer or the new one, a
    /// racer rebuilding the same revision at worst overwrites it with an
    /// equally current layer, and a file that vanished meanwhile is simply
    /// written. A denied write keeps `layer` in memory, as in
    /// [`Self::publish_base`]; a write that reaches the disk drops the layer
    /// an earlier denied write of this process kept.
    pub fn replace_base(&self, revision: &str, layer: &GraphLayer) -> Result<()> {
        self.write_or_fall_back(&self.base_path(revision), layer)
            .map(|_| ())
    }

    /// Read a stage's overlay, or `None` when it has none.
    pub fn load_overlay(&self, plan: &str, stage: &str) -> Result<Option<GraphLayer>> {
        self.read_layer_or_memory(&self.overlay_path(plan, stage))
    }

    /// Write a stage's overlay, replacing any previous one.
    ///
    /// Overlays are mutable — a stage rewrites its own as it edits — but they
    /// are private to that stage, so no other reader can observe a torn write.
    pub fn save_overlay(&self, plan: &str, stage: &str, layer: &GraphLayer) -> Result<()> {
        let mut persisted = layer.clone();
        let base = self.load_base(&layer.revision)?;
        persisted.files.retain(|path, entry| {
            entry.coverage != FileCoverage::Deleted
                || base
                    .as_ref()
                    .is_some_and(|base| base.files.contains_key(path))
        });
        self.write_or_fall_back(&self.overlay_path(plan, stage), &persisted)
            .map(|_| ())
    }

    /// Delete a stage's overlay layer file. Idempotent.
    ///
    /// Called after the stage's work is merged and folded into a new base
    /// layer, at which point the overlay layer describes a revision nobody
    /// reads. Removes only [`LAYER_FILE`] and the overlay's resolved view
    /// (`view.json`) — never the overlay directory — because that directory is a shared namespace, not this module's alone:
    /// `crate::commands::context::record_edit` keeps `dirty-paths.json` there,
    /// and `crate::context::delivery` keeps `session-retrieval/*.json` there,
    /// and both outlive the graph layer and are read by other stages after
    /// this one merges. A `remove_dir_all` here would delete those out from
    /// under their owners on a schedule this module does not control; `.loom/work/`
    /// is removed wholesale when the plan finishes, so the leftover directory
    /// does not accumulate across plans.
    pub fn discard_overlay(&self, plan: &str, stage: &str) -> Result<()> {
        let path = self.overlay_path(plan, stage);
        self.discard_overlay_view(plan, stage);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error)
                .with_context(|| format!("Failed to remove overlay layer: {}", path.display())),
        }
    }

    /// Resolve a stage overlay against its recorded base, or `revision` when
    /// no overlay exists. A legacy overlay without a revision has no base.
    ///
    /// Pass `None` for the stage to read the base layer alone. A missing base
    /// is not an error — an overlay-only view is exactly what a stage sees
    /// before the host has ever published a base.
    pub fn resolved(&self, revision: &str, stage: Option<(&str, &str)>) -> Result<ResolvedGraph> {
        self.resolved_with_schema(revision, stage)
            .map(|(resolved, _)| resolved)
    }

    /// [`Self::resolved`], and whether every layer it drew from was written
    /// under the current schema ([`GraphLayer::has_current_schema`]): `load_base`
    /// and `load_overlay` serve a layer of another schema unfiltered. A missing
    /// layer draws nothing, so it counts as current.
    pub(crate) fn resolved_with_schema(
        &self,
        revision: &str,
        stage: Option<(&str, &str)>,
    ) -> Result<(ResolvedGraph, bool)> {
        let read = self.graph_read(revision, stage)?;
        self.resolve_read(&read)
    }

    pub(crate) fn graph_read<'a>(
        &self,
        revision: &str,
        address: Option<(&'a str, &'a str)>,
    ) -> Result<GraphRead<'a>> {
        let overlay = match address {
            Some((plan, stage)) => self.load_overlay(plan, stage)?,
            None => None,
        };
        let revision = overlay.as_ref().map_or(revision, |layer| &layer.revision);
        Ok(GraphRead {
            revision: revision.to_owned(),
            address: address.filter(|_| overlay.is_some()),
            overlay,
        })
    }

    pub(crate) fn resolve_read(&self, read: &GraphRead<'_>) -> Result<(ResolvedGraph, bool)> {
        let base = if read.revision.is_empty() {
            None
        } else {
            self.load_base(&read.revision)?
                .filter(|base| base.revision == read.revision)
        };
        let mut current = base.as_ref().is_none_or(GraphLayer::has_current_schema);
        let base = base.unwrap_or_default();
        let mut resolved = ResolvedGraph {
            base_revision: base.revision.clone(),
            overlaid: BTreeSet::new(),
            files: base.files,
        };

        if let Some(overlay) = &read.overlay {
            current &= overlay.has_current_schema();
            for (path, entry) in &overlay.files {
                // Wholesale replacement, never a merge: an overlay entry is
                // the complete truth for that file in this stage.
                if entry.coverage == FileCoverage::Deleted {
                    resolved.files.remove(path);
                } else {
                    resolved.files.insert(path.clone(), entry.clone());
                    resolved.overlaid.insert(path.clone());
                }
            }
        }

        Ok((resolved, current))
    }
}

/// Read one layer file, treating absence as "never written" and a file that
/// does not parse as absent too: a truncated or foreign-format layer must be
/// rebuilt, never wedge the cache.
fn read_layer(path: &Path) -> Result<Option<GraphLayer>> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("Failed to read source graph: {}", path.display()));
        }
    };

    match serde_json::from_str(&content) {
        Ok(layer) => Ok(Some(layer)),
        Err(error) => {
            tracing::warn!(
                path = %path.display(),
                error = %error,
                "discarding unparseable source graph layer"
            );
            Ok(None)
        }
    }
}

/// Write one layer file with a locked, crash-atomic replacement.
fn write_layer(path: &Path, layer: &GraphLayer) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "Failed to create source graph directory: {}",
                parent.display()
            )
        })?;
    }
    let content = canonical_json(layer)?;
    crate::fs::locking::locked_write(path, &content)
        .with_context(|| format!("Failed to write source graph: {}", path.display()))
}

mod fallback;
mod layer_types;
mod prune;

pub(crate) use fallback::is_write_denied;
pub use layer_types::{FileEntry, GraphLayer, ResolvedGraph};

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_corrupt;
