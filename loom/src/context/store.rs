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
