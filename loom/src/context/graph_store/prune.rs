//! Base graph GC (`doc/PROPOSAL-retrieval-precision.md` §A.14):
//! [`GraphStore::prune_base_graphs`], plus the small helpers
//! [`GraphStore::publish_base`] needs to call it with the right `keep` count
//! and protected revision. A base's resolved views (`graph/view/<revision>-*.json`)
//! leave with the base, and [`GraphStore::prune_to_budget`] keeps `graph/base`
//! plus `graph/view` under `RetrievalConfig::graph_cache_budget_bytes`.
//!
//! Split out of `graph_store/mod.rs` to keep that file under the 400-line
//! cap — the same reason `refresh/semantic.rs` was split out of `refresh.rs`.

use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::{GraphStore, VIEW_RELATIVE_DIR};
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
    /// [`RetrievalConfig::load`]; `None` degrades [`Self::retrieval_config`]
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

    /// The retrieval tunables of the project this store's cache lives under,
    /// or the compiled-in defaults when the project root cannot be derived —
    /// `RetrievalConfig::load` itself never fails on a missing or unparseable
    /// file, so this never needs to either.
    fn retrieval_config(&self) -> RetrievalConfig {
        match self.derive_project_root() {
            Some(root) => RetrievalConfig::load(&root),
            None => RetrievalConfig::default(),
        }
    }

    /// Prune after publishing, protecting the new and current semantic revisions.
    pub(super) fn prune_after_publish(&self, just_written_revision: &str) {
        let config = self.retrieval_config();
        let current = self.current_semantic_revision();
        let mut protected: Vec<&str> = vec![just_written_revision];
        if let Some(current) = current.as_deref() {
            protected.push(current);
        }
        // A view of a revision that is only now being published was built
        // from something else, so it is never served.
        self.remove_views(just_written_revision);
        if let Err(error) = self.prune_base_graphs(config.keep_base_graphs, &protected) {
            tracing::debug!(%error, "base graph prune after publish failed");
        }
        if let Err(error) = self.prune_to_budget(config.graph_cache_budget_bytes, &protected) {
            tracing::debug!(%error, "graph cache budget prune after publish failed");
        }
    }

    /// Prune published base layers, keeping the most useful few.
    ///
    /// Retains every stem named in `protected` — from
    /// `Self::prune_after_publish` this is the revision just written plus
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
            if let Some(revision) = path.file_stem().and_then(|stem| stem.to_str()) {
                self.remove_views(revision);
            }
        }

        Ok(())
    }

    /// Evict whole revisions (base plus views), oldest first, until `graph/base`
    /// and `graph/view` together fit `budget` bytes. A revision named in
    /// `protected` is never evicted, so the total can stay above the budget
    /// when only protected revisions remain. Best-effort like
    /// [`Self::prune_base_graphs`]: an unlink failure is logged, not returned.
    pub fn prune_to_budget(&self, budget: usize, protected: &[&str]) -> Result<()> {
        let mut usage: BTreeMap<String, RevisionUsage> = BTreeMap::new();
        for file in list_json(&self.base_dir())? {
            let Some(revision) = file.path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            let entry = usage.entry(revision.to_string()).or_default();
            entry.bytes += file.bytes;
            entry.base_modified = Some(file.modified);
        }
        for file in list_json(&self.view_dir())? {
            let Some(revision) = view_revision(&file.path) else {
                continue;
            };
            let entry = usage.entry(revision.to_string()).or_default();
            entry.bytes += file.bytes;
            entry.view_modified = entry.view_modified.max(Some(file.modified));
        }

        let mut total: u64 = usage.values().map(|entry| entry.bytes).sum();
        let mut evictable: Vec<(&String, &RevisionUsage)> = usage
            .iter()
            .filter(|(revision, _)| !protected.contains(&revision.as_str()))
            .collect();
        evictable.sort_by_key(|(_, entry)| entry.age_key());
        for (revision, entry) in evictable {
            if total <= budget as u64 {
                break;
            }
            if let Err(error) = fs::remove_file(self.base_path(revision)) {
                if error.kind() != ErrorKind::NotFound {
                    tracing::debug!(revision, %error, "failed to evict base graph layer");
                }
            }
            self.remove_views(revision);
            total = total.saturating_sub(entry.bytes);
        }
        Ok(())
    }

    /// Directory holding the resolved views of base revisions.
    pub(crate) fn view_dir(&self) -> PathBuf {
        self.graph_root.join(VIEW_RELATIVE_DIR)
    }

    /// Every persisted view file of `revision`, whatever identity it carries.
    pub(crate) fn view_files(&self, revision: &str) -> Vec<PathBuf> {
        let files = list_json(&self.view_dir()).unwrap_or_default();
        files
            .into_iter()
            .map(|file| file.path)
            .filter(|path| view_revision(path) == Some(revision))
            .collect()
    }

    /// Every persisted view of a revision other than `revision`, newest first.
    pub(crate) fn older_view_files(&self, revision: &str) -> Vec<PathBuf> {
        let mut files = list_json(&self.view_dir()).unwrap_or_default();
        files.retain(|file| view_revision(&file.path).is_some_and(|other| other != revision));
        files.sort_by_key(|file| std::cmp::Reverse(file.modified));
        files.into_iter().map(|file| file.path).collect()
    }

    /// Delete every persisted view of `revision`, and forget the ones this
    /// process holds in memory: they describe the layer being replaced or
    /// pruned. Best-effort.
    pub(crate) fn remove_views(&self, revision: &str) {
        let view_dir = self.view_dir();
        let of_revision = |path: &Path| {
            path.parent() == Some(view_dir.as_path()) && view_revision(path) == Some(revision)
        };
        for held in [&self.view_cache, &self.view_fallback] {
            held.borrow_mut()
                .retain(|path, _| !of_revision(path.as_path()));
        }
        for path in self.view_files(revision) {
            if let Err(error) = fs::remove_file(&path) {
                tracing::debug!(path = %path.display(), %error, "failed to remove resolved view");
            }
        }
    }
}

/// One `*.json` file of a cache directory.
struct JsonFile {
    path: PathBuf,
    bytes: u64,
    modified: SystemTime,
}

/// What one revision occupies in `graph/base` and `graph/view`.
#[derive(Default)]
struct RevisionUsage {
    bytes: u64,
    /// Modification time of the base layer, when one exists.
    base_modified: Option<SystemTime>,
    view_modified: Option<SystemTime>,
}

impl RevisionUsage {
    /// Publication time of the base, or the newest view when only views remain.
    fn age_key(&self) -> SystemTime {
        self.base_modified
            .or(self.view_modified)
            .unwrap_or(SystemTime::UNIX_EPOCH)
    }
}

/// The `*.json` files of `dir`; a missing directory holds none.
fn list_json(dir: &Path) -> Result<Vec<JsonFile>> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error).with_context(|| format!("Failed to list {}", dir.display()));
        }
    };
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry.with_context(|| format!("Failed to read entry in {}", dir.display()))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        files.push(JsonFile {
            path,
            bytes: metadata.len(),
            modified: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
        });
    }
    Ok(files)
}

/// The revision of a `<revision>-<identity digest>.json` view file name.
fn view_revision(path: &Path) -> Option<&str> {
    let stem = path.file_stem()?.to_str()?;
    stem.rsplit_once('-').map(|(revision, _)| revision)
}
