//! File-backed storage for derived context artifacts.
//!
//! This cache is derived and rebuildable. It is resolved through
//! [`WorkDir::main_project_root`] so linked Git worktrees share one cache rather
//! than each worktree growing its own immediately-stale copy.

use crate::context::schema::Freshness;
use crate::fs::knowledge::catalog::Catalog;
use crate::fs::work_dir::WorkDir;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// Cache directory relative to the canonical main project root.
pub const CACHE_RELATIVE_DIR: &str = ".loom/cache/context-v1";
/// File name of the persisted chunk catalog.
pub const CATALOG_FILE: &str = "catalog.json";
/// File name of the persisted freshness state.
pub const STATE_FILE: &str = "state.json";

/// Persisted freshness of each derived layer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoreState {
    /// Freshness of the structurally-derived catalog layer.
    #[serde(default)]
    pub structural: Freshness,
    /// Freshness of the semantically-derived source layer.
    #[serde(default)]
    pub semantic: Freshness,
    /// Identity hash of the catalog that `structural.revision` describes.
    #[serde(default)]
    pub catalog_revision: String,
}

/// Handle on the derived-artifact cache directory.
#[derive(Debug, Clone)]
pub struct ContextStore {
    root: PathBuf,
}

impl ContextStore {
    /// Resolve the cache under the canonical MAIN project root.
    ///
    /// In a linked worktree `.work` is a symlink into the main repository, so
    /// `main_project_root` follows it. Resolving relative to the worktree instead
    /// would give every parallel stage a private, immediately-stale cache.
    pub fn open(work_dir: &WorkDir) -> Result<Self> {
        let main_project_root = work_dir.main_project_root().ok_or_else(|| {
            anyhow::anyhow!(
                "Could not resolve the canonical main project root for the context cache"
            )
        })?;

        Ok(Self::with_root(main_project_root.join(CACHE_RELATIVE_DIR)))
    }

    /// Construct directly from a cache root. Used by tests.
    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Return the cache root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Create the cache directory tree. Idempotent.
    pub fn ensure(&self) -> Result<()> {
        fs::create_dir_all(&self.root).with_context(|| {
            format!(
                "Failed to create context cache directory: {}",
                self.root.display()
            )
        })
    }

    /// Return the persisted catalog path.
    pub fn catalog_path(&self) -> PathBuf {
        self.root.join(CATALOG_FILE)
    }

    /// Return the persisted freshness-state path.
    pub fn state_path(&self) -> PathBuf {
        self.root.join(STATE_FILE)
    }

    /// Load the persisted catalog, if it has been written.
    pub fn load_catalog(&self) -> Result<Option<Catalog>> {
        let path = self.catalog_path();
        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("Failed to read context catalog: {}", path.display())
                });
            }
        };

        serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse context catalog: {}", path.display()))
            .map(Some)
    }

    /// Persist the catalog with a locked, crash-atomic replacement.
    pub fn save_catalog(&self, catalog: &Catalog) -> Result<()> {
        self.ensure()?;
        let path = self.catalog_path();
        let content = canonical_json(catalog)?;
        crate::fs::locking::locked_write(&path, &content)
            .with_context(|| format!("Failed to write context catalog: {}", path.display()))
    }

    /// Load the persisted freshness state, or the default state if absent.
    pub fn load_state(&self) -> Result<StoreState> {
        let path = self.state_path();
        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(StoreState::default()),
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("Failed to read context state: {}", path.display()));
            }
        };

        serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse context state: {}", path.display()))
    }

    /// Persist freshness state with a locked, crash-atomic replacement.
    pub fn save_state(&self, state: &StoreState) -> Result<()> {
        self.ensure()?;
        let path = self.state_path();
        let content = canonical_json(state)?;
        crate::fs::locking::locked_write(&path, &content)
            .with_context(|| format!("Failed to write context state: {}", path.display()))
    }

    /// Read-modify-write `state.json` as one critical section (see
    /// doc/loom/knowledge/patterns.md § A-5, "Locked Stage Read-Modify-Write
    /// Pattern"): the directory lock is held from before the read to after
    /// the write, so `mutate` sees the value as it is *right now*, not a
    /// snapshot a caller took earlier. `mutate` can therefore only ever
    /// clobber the field(s) it explicitly assigns — every other field keeps
    /// whatever a concurrent writer already put there. Loading outside the
    /// lock (or writing back a whole `StoreState` captured before it) is the
    /// lost update this exists to prevent.
    ///
    /// `mutate` MUST be fast and touch only `state.json`: the lock is
    /// exclusive and not reentrant — `fs2`'s `flock` is scoped to the open
    /// file description, not the process, so a nested lock attempt on this
    /// same directory (e.g. calling [`Self::save_state`] from inside
    /// `mutate`) blocks forever on itself. Slow work (`ingest`, a repo scan,
    /// ...) belongs BEFORE the call; only its result is assigned in `mutate`.
    ///
    /// A `state.json` that is missing, unreadable, or malformed degrades to
    /// [`StoreState::default`] rather than failing the call, so a broken
    /// cache can never block the write that would otherwise repair it. The
    /// fallback is not silent: it is logged via `tracing::warn!` with the
    /// path and the error before `mutate` runs, so this reads as "tolerated
    /// and reported", not "tolerated and hidden".
    pub(crate) fn update_state<F>(&self, mutate: F) -> Result<()>
    where
        F: FnOnce(&mut StoreState),
    {
        self.ensure()?;
        let path = self.state_path();
        crate::fs::locking::locked_dir_update(&self.root, || {
            let mut state = self.load_state().unwrap_or_else(|error| {
                tracing::warn!(
                    path = %path.display(),
                    %error,
                    "context state.json unreadable; falling back to a default and repairing it"
                );
                StoreState::default()
            });
            mutate(&mut state);
            let content = canonical_json(&state)?;
            crate::fs::locking::atomic_write_locked(&path, &content)
                .with_context(|| format!("Failed to write context state: {}", path.display()))
        })
    }
}

/// Serialize deterministically: identical input values produce byte-identical output.
pub fn canonical_json<T: Serialize>(value: &T) -> Result<String> {
    // Any map inside a persisted type must be a BTreeMap, never a HashMap, or
    // output stops being byte-identical across runs.
    let mut json =
        serde_json::to_string_pretty(value).context("Failed to serialize context cache JSON")?;
    json.push('\n');
    Ok(json)
}
