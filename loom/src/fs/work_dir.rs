use anyhow::{bail, Context, Result};
use std::fs;
use std::os::unix::fs::DirBuilderExt;
#[cfg(test)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use toml_edit::DocumentMut;

use crate::fs::knowledge::KnowledgeDir;
use crate::models::session::TerminalConfig;
use crate::plan::schema::SandboxConfig;
use crate::remote_control::RemoteControlConfig;

mod context_config;
pub use context_config::ContextConfig;

/// Parsed config.toml structure
#[derive(Debug, Clone)]
pub struct Config {
    inner: toml::Value,
}

impl Config {
    /// Get a string value from the plan section (e.g., "source_path", "base_branch", "plan_id")
    pub fn get_plan_str(&self, key: &str) -> Option<&str> {
        self.inner
            .get("plan")
            .and_then(|p| p.get(key))
            .and_then(|v| v.as_str())
    }

    /// Get the plan source path
    pub fn source_path(&self) -> Option<PathBuf> {
        self.get_plan_str("source_path").map(PathBuf::from)
    }

    /// Get the base branch for merging
    pub fn base_branch(&self) -> Option<String> {
        self.get_plan_str("base_branch").map(String::from)
    }

    /// Get the plan ID
    pub fn plan_id(&self) -> Option<&str> {
        self.get_plan_str("plan_id")
    }

    /// Get mutable access to the underlying TOML value for updates
    pub fn as_toml_mut(&mut self) -> &mut toml::Value {
        &mut self.inner
    }

    /// Serialize the config back to TOML string
    pub fn to_toml_string(&self) -> Result<String> {
        toml::to_string_pretty(&self.inner).context("Failed to serialize config")
    }
}

/// Load and parse config.toml from a work directory
///
/// # Arguments
/// * `work_dir` - Path to the `.loom/work` directory (not the config file itself)
///
/// # Returns
/// * `Ok(Some(Config))` - Config loaded and parsed successfully
/// * `Ok(None)` - Config file doesn't exist
/// * `Err(_)` - Failed to read or parse config
pub fn load_config(work_dir: &Path) -> Result<Option<Config>> {
    let config_path = work_dir.join("config.toml");

    if !config_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&config_path).context("Failed to read config.toml")?;

    let inner: toml::Value = toml::from_str(&content).context("Failed to parse config.toml")?;

    Ok(Some(Config { inner }))
}

/// Load config.toml, returning an error if it doesn't exist
///
/// Use this when config.toml is required (e.g., during execution).
pub fn load_config_required(work_dir: &Path) -> Result<Config> {
    load_config(work_dir)?
        .ok_or_else(|| anyhow::anyhow!("No active plan. Run 'loom init <plan-path>' first."))
}

/// Directory holding loom's per-project data; `work/` is one of its children,
/// `cache/` (written by `loom map`) another.
const LOOM_DIR: &str = ".loom";
/// The state root's own name under [`LOOM_DIR`].
const WORK_DIR: &str = "work";
/// The pre-move spelling of the state root, directly under the project root.
const LEGACY_WORK_DIR: &str = ".work";

/// Which on-disk spelling a resolved workspace uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// `<repo>/.loom/work` — the current layout.
    Nested,
    /// `<repo>/.work` — a workspace created before the move.
    Legacy,
}

/// The workspace rooted at `dir`, if either layout has a `config.toml` there.
///
/// Keyed on the config FILE, never on directory existence: `~/.loom/config.toml`
/// is a user-level file and `.loom/cache/` appears in any project that has run
/// `loom map`, so a bare `.loom/` marks nothing. Nested wins over legacy when
/// both are present.
fn workspace_at(dir: &Path) -> Option<(PathBuf, Layout)> {
    let nested = dir.join(LOOM_DIR).join(WORK_DIR);
    if nested.join("config.toml").exists() {
        return Some((nested, Layout::Nested));
    }
    let legacy = dir.join(LEGACY_WORK_DIR);
    if legacy.join("config.toml").exists() {
        return Some((legacy, Layout::Legacy));
    }
    None
}

/// The layout `base` names when it already IS a state root rather than a
/// project root, in either spelling.
///
/// Hook entry points (see `commands/hook/reconcile_graph.rs`) hand `WorkDir::new`
/// `LOOM_WORK_DIR`, which names the state directory ITSELF, not its parent — so
/// a `base` that already names one must resolve to itself rather than get a
/// second state root appended under it. Both spellings need recognising: after
/// the move the pinned value ends `.loom/work`, whose final component alone is
/// the unremarkable `work`, while a workspace created before the move still
/// pins a single `.work`. Miss either and a stale pin materializes a phantom
/// `<...>/.loom/work/.loom/work` (or `<...>/.work/.work`), whose `repo_root()`
/// is the state directory itself. `initialize()` creates the root this returns,
/// so the branch keeps that creation correct for a state-root-named hint too.
fn base_names_state_root(base: &Path) -> Option<Layout> {
    let name = base.file_name()?;
    if name == std::ffi::OsStr::new(WORK_DIR)
        && base.parent().and_then(Path::file_name) == Some(std::ffi::OsStr::new(LOOM_DIR))
    {
        return Some(Layout::Nested);
    }
    if name == std::ffi::OsStr::new(LEGACY_WORK_DIR) {
        return Some(Layout::Legacy);
    }
    None
}

/// The nearest ancestor of `dir` (inclusive) holding a `.git` entry, or `None`
/// when there is none.
///
/// This is the bound on the upward workspace search: a `.git` marks the one
/// tree whose `config.toml` can legitimately be this base's workspace. `.git`
/// is an ENTRY, not necessarily a directory — a linked worktree's is a file
/// pointing at the main repo's gitdir — so existence, not `is_dir`, is the
/// test.
fn nearest_git_root(dir: &Path) -> Option<&Path> {
    let mut current = dir;
    loop {
        if current.join(".git").exists() {
            return Some(current);
        }
        match current.parent() {
            Some(parent) if parent != current => current = parent,
            _ => return None,
        }
    }
}

/// Apply the layout's hop count to a state-root path.
fn repo_root_of(root: &Path, layout: Layout) -> Option<&Path> {
    match layout {
        Layout::Nested => root.parent()?.parent(),
        Layout::Legacy => root.parent(),
    }
}

pub struct WorkDir {
    root: PathBuf,
    layout: Layout,
}

impl WorkDir {
    /// Resolve the workspace for `base_path`, picking the root exactly once.
    ///
    /// Whatever root resolves is the workspace for reads AND writes: a project
    /// still on `.work/` keeps getting its signals, stages and config writes
    /// there. Nothing here ever produces a `.work/` that does not already
    /// exist — the fallback is always the nested layout, so `initialize()` on a
    /// fresh repo lands at `.loom/work`.
    pub fn new<P: AsRef<Path>>(base_path: P) -> Result<Self> {
        let base = base_path.as_ref();
        // `base` itself first, uncanonicalized: callers passing "." (see
        // `commands/status.rs`) get a root relative to it, as they always have.
        if let Some((root, layout)) = workspace_at(base) {
            return Ok(Self { root, layout });
        }

        // Search upward for a workspace (like git searches for .git), bounded
        // at the enclosing repository: the walk covers `base` up to and
        // including the nearest ancestor holding a `.git`, and NO base outside
        // a repository walks at all. Without that second half the walk ran to
        // `/`, inspecting `$HOME` and the OS temp root on the way, and silently
        // adopted the first `config.toml` it met — so one `loom init` in
        // `$HOME` would claim every later command issued from any non-git
        // directory beneath it, for writes as well as reads.
        if let Ok(abs) = base.canonicalize() {
            if let Some(repo_root) = nearest_git_root(&abs) {
                let mut current = abs.as_path();
                loop {
                    if let Some((root, layout)) = workspace_at(current) {
                        return Ok(Self { root, layout });
                    }
                    if current == repo_root {
                        break;
                    }
                    match current.parent() {
                        Some(parent) if parent != current => current = parent,
                        _ => break,
                    }
                }
            }
        }

        if let Some(layout) = base_names_state_root(base) {
            return Ok(Self {
                root: base.to_path_buf(),
                layout,
            });
        }

        Ok(Self {
            root: base.join(LOOM_DIR).join(WORK_DIR),
            layout: Layout::Nested,
        })
    }

    /// The spelling of the resolved state root.
    pub fn layout(&self) -> Layout {
        self.layout
    }

    /// Open an existing state directory or initialise it if missing.
    ///
    /// Used by `loom init` reconfigure paths so a second invocation (with
    /// different flags) does not destroy existing state (per finding #11).
    pub fn open_or_initialize(&self) -> Result<()> {
        if self.root.exists() {
            // Already initialised — validate structure and return.
            self.load()
        } else {
            self.initialize()
        }
    }

    pub fn initialize(&self) -> Result<()> {
        if self.root.exists() {
            bail!("{} already exists", self.root.display());
        }

        // Recursive: the nested layout's `.loom/` parent may not exist yet.
        let mut root_builder = fs::DirBuilder::new();
        root_builder.recursive(true).mode(0o700);
        root_builder
            .create(&self.root)
            .with_context(|| format!("Failed to create {}", self.root.display()))?;

        self.ensure_layout()
    }

    /// Adopt a state directory that already exists on disk but holds no
    /// orchestration state — e.g. a phantom directory a stale `LOOM_WORK_DIR`
    /// pin caused a hook to materialize against a since-deleted workspace.
    /// `commands/init/execute.rs` decides when that is actually safe (no
    /// `config.toml`, no stage/session files) before calling this.
    ///
    /// Requires `self.root` to already exist — bails otherwise, unlike
    /// `initialize()`, which requires the opposite and creates it. Never
    /// creates the root itself, so a caller's `InitGuard` must not delete a
    /// directory this call did not create.
    pub fn adopt_existing(&self) -> Result<()> {
        if !self.root.exists() {
            bail!("{} does not exist; nothing to adopt", self.root.display());
        }

        self.ensure_layout()
    }

    /// The layout work both `initialize()` and `adopt_existing()` need once
    /// `self.root` exists: the private-mode subdirectories, the README, and
    /// the knowledge directory. Idempotent — skips anything already present,
    /// so `adopt_existing()` can call it against a state root that already
    /// holds some (but not all) of this layout.
    fn ensure_layout(&self) -> Result<()> {
        // Includes `memory`, `wrappers`, `pids` — session wrapper scripts,
        // PID tracking files, and the memory journal all live under these.
        let subdirs = [
            "signals", "handoffs", "archive", "stages", "sessions", "crashes", "memory",
            "wrappers", "pids",
        ];

        for subdir in &subdirs {
            let path = self.root.join(subdir);
            if path.exists() {
                continue;
            }
            let mut builder = fs::DirBuilder::new();
            builder.mode(0o700);
            builder
                .create(&path)
                .with_context(|| format!("Failed to create {subdir} directory"))?;
        }

        if !self.root.join("README.md").exists() {
            self.create_readme()?;
        }

        // Initialize knowledge directory with template files.
        // KnowledgeDir expects the project root, not the state root — the hop
        // count differs per layout, which is why this goes through
        // `project_root()` rather than a bare `parent()`.
        if let Some(project_root) = self.project_root() {
            let knowledge = KnowledgeDir::new(project_root);
            knowledge.initialize()?;
        }

        Ok(())
    }

    pub fn load(&self) -> Result<()> {
        if !self.root.exists() {
            bail!(
                "{} does not exist. Run 'loom init' first.",
                self.root.display()
            );
        }

        self.validate_structure()?;

        Ok(())
    }

    fn validate_structure(&self) -> Result<()> {
        let required_dirs = [
            "signals", "handoffs", "archive", "stages", "sessions", "crashes", "memory",
            "wrappers", "pids",
        ];

        for dir in &required_dirs {
            let path = self.root.join(dir);
            if !path.exists() {
                // Auto-create missing directories instead of failing, at the
                // same 0o700 mode `initialize()`/`ensure_layout()` use — a
                // plain `create_dir` would land these at the process umask
                // instead, inside an otherwise-0700 state root.
                let mut builder = fs::DirBuilder::new();
                builder.mode(0o700);
                builder
                    .create(&path)
                    .with_context(|| format!("Failed to create missing directory: {dir}"))?;
            }
        }

        Ok(())
    }

    fn create_readme(&self) -> Result<()> {
        let readme_content = r#"# loom Work Directory

This directory is managed by loom CLI and contains:

- `signals/` - Inter-agent communication
- `handoffs/` - Context handoff records
- `archive/` - Archived entities
- `stages/` - Stage definitions and status
- `sessions/` - Active session tracking
- `crashes/` - Crash reports and diagnostics
- `knowledge/` - Curated codebase knowledge (entry points, patterns, conventions)

Do not manually edit these files unless you know what you're doing.
"#;

        let readme_path = self.root.join("README.md");
        fs::write(readme_path, readme_content).context("Failed to create README.md")?;

        Ok(())
    }

    pub fn signals_dir(&self) -> PathBuf {
        self.root.join("signals")
    }

    pub fn handoffs_dir(&self) -> PathBuf {
        self.root.join("handoffs")
    }

    pub fn archive_dir(&self) -> PathBuf {
        self.root.join("archive")
    }

    pub fn stages_dir(&self) -> PathBuf {
        self.root.join("stages")
    }

    pub fn sessions_dir(&self) -> PathBuf {
        self.root.join("sessions")
    }

    pub fn crashes_dir(&self) -> PathBuf {
        self.root.join("crashes")
    }

    pub fn knowledge_dir(&self) -> PathBuf {
        self.root.join("knowledge")
    }

    /// Path to `.loom/work/disputes/` — adjudication artifacts. See
    /// `models/dispute.rs` for the per-id directory schema.
    pub fn disputes_dir(&self) -> PathBuf {
        self.root.join("disputes")
    }

    /// Path to `.loom/work/plan_versions/` — plan amendment snapshots and
    /// audit log. Populated by the Stage 3 plan-amendment pipeline.
    pub fn plan_versions_dir(&self) -> PathBuf {
        self.root.join("plan_versions")
    }

    /// Get the config.toml path
    pub fn config_path(&self) -> PathBuf {
        self.root.join("config.toml")
    }

    /// Ensure a subdirectory exists, creating it if needed
    ///
    /// # Arguments
    /// * `name` - The subdirectory name relative to the state root
    ///
    /// # Returns
    /// The full path to the directory
    pub fn ensure_dir(&self, name: &str) -> Result<PathBuf> {
        let dir = self.root.join(name);
        fs::create_dir_all(&dir).with_context(|| format!("Failed to create {name} directory"))?;
        Ok(dir)
    }

    /// Load and parse config.toml
    ///
    /// # Returns
    /// * `Ok(Some(Config))` - Config loaded and parsed successfully
    /// * `Ok(None)` - Config file doesn't exist
    /// * `Err(_)` - Failed to read or parse config
    pub fn load_config(&self) -> Result<Option<Config>> {
        load_config(&self.root)
    }

    /// Load config.toml, returning an error if it doesn't exist
    pub fn load_config_required(&self) -> Result<Config> {
        load_config_required(&self.root)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The project root holding this state directory, layout-aware.
    ///
    /// Two hops up from `.loom/work`, one from a legacy `.work`. Every caller
    /// goes through here (or [`Self::project_root`], its alias) so the hop
    /// count exists in exactly one place.
    pub fn repo_root(&self) -> Option<&Path> {
        repo_root_of(&self.root, self.layout)
    }

    /// Get the project root.
    pub fn project_root(&self) -> Option<&Path> {
        self.repo_root()
    }

    /// Get the main project root by following symlinks.
    ///
    /// In a worktree the state root is a symlink into the main repo's:
    /// `.loom/work -> ../../../.loom/work`, or `.work -> ../../.work` under the
    /// legacy layout. This method resolves that symlink to find the true main
    /// repository root.
    ///
    /// - If the state root is a symlink, follows it and applies the layout's
    ///   hop count to the resolved path.
    /// - Otherwise returns the regular project root (same as `project_root()`).
    pub fn main_project_root(&self) -> Option<PathBuf> {
        if self.root.is_symlink() {
            // Read the symlink target
            if let Ok(link_target) = fs::read_link(&self.root) {
                // If the symlink is relative, resolve it against the directory
                // holding the link itself.
                let resolved = if link_target.is_relative() {
                    self.root.parent()?.join(&link_target)
                } else {
                    link_target
                };

                // Canonicalize to get the absolute path
                if let Ok(canonical) = resolved.canonicalize() {
                    return repo_root_of(&canonical, self.layout).map(|p| p.to_path_buf());
                }
            }
            None
        } else {
            // Not a symlink, return regular project root
            self.project_root().map(|p| p.to_path_buf())
        }
    }
}

// ==========================================================================
// Centralized .loom/work/config.toml API
//
// All read/write to `.loom/work/config.toml` MUST go through this module so that:
//   * comments and unknown keys are preserved (toml_edit, not toml::Value),
//   * structured sub-tables (`[plan_sandbox]`) have one canonical location,
//   * concurrent access serializes through the file lock used by other
//     `fs/` writers when needed by callers.
//
// Section layout in `.loom/work/config.toml`:
//
//   [plan]
//   source_path / plan_id / plan_name / base_branch
//
//   [plan_sandbox]   # persisted snapshot of plan-level sandbox at init time
//
// Section keys for the persisted plan-level config (see `read_plan_sandbox`).
// ==========================================================================

const PLAN_SANDBOX_SECTION: &str = "plan_sandbox";
const REMOTE_CONTROL_SECTION: &str = "remote_control";
const TERMINAL_SECTION: &str = "terminal";
const CONTEXT_SECTION: &str = "context";

fn config_path(work_dir: &Path) -> PathBuf {
    work_dir.join("config.toml")
}

/// Read `.loom/work/config.toml` as a `toml_edit::DocumentMut`, preserving
/// comments, formatting, and unknown keys. Returns an empty document if the
/// file does not exist.
pub fn read_config(work_dir: &Path) -> Result<DocumentMut> {
    let path = config_path(work_dir);
    if !path.exists() {
        return Ok(DocumentMut::new());
    }
    let content =
        fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))?;
    content
        .parse::<DocumentMut>()
        .with_context(|| format!("Failed to parse {}", path.display()))
}

/// Write the document back to `.loom/work/config.toml`, crash-atomically and
/// under the state directory lock.
///
/// The write goes through [`crate::fs::locking::locked_write`] (temp file +
/// `fsync` + `rename`), so a crash mid-write leaves either the old config or the
/// fully-written new config — never a truncated file. The lock serializes
/// against other config writers using the same module.
///
/// NOTE: callers performing a read-modify-write (`read_config` → mutate →
/// `write_config`) should prefer [`update_config`], which holds the lock across
/// the entire sequence so concurrent writers cannot lose each other's sections.
/// A bare `write_config` only makes the final write atomic, not the surrounding
/// read-modify-write.
pub fn write_config(work_dir: &Path, doc: &DocumentMut) -> Result<()> {
    let path = config_path(work_dir);
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create {}", parent.display()))?;
        }
    }
    crate::fs::locking::locked_write(&path, &doc.to_string())
        .with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

/// Read-modify-write `.loom/work/config.toml` while holding the state
/// directory lock for the whole sequence.
///
/// This is the lost-update-safe way to mutate the config: the read, the
/// `modify` closure, and the atomic write all happen under a single exclusive
/// directory lock, so a concurrent daemon plan-rename and a CLI section write
/// can no longer interleave and drop each other's sections.
pub fn update_config<F>(work_dir: &Path, modify: F) -> Result<()>
where
    F: FnOnce(&mut DocumentMut) -> Result<()>,
{
    let path = config_path(work_dir);
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create {}", parent.display()))?;
        }
    }

    crate::fs::locking::locked_update(&path, |existing| {
        let mut doc = if existing.is_empty() {
            DocumentMut::new()
        } else {
            existing
                .parse::<DocumentMut>()
                .with_context(|| format!("Failed to parse {}", path.display()))?
        };
        modify(&mut doc)?;
        Ok(doc.to_string())
    })
    .with_context(|| format!("Failed to update {}", path.display()))
}

fn read_section<T: serde::de::DeserializeOwned>(
    work_dir: &Path,
    section: &str,
) -> Result<Option<T>> {
    let path = config_path(work_dir);
    if !path.exists() {
        return Ok(None);
    }
    let content =
        fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))?;
    let value: toml::Value = toml::from_str(&content)
        .with_context(|| format!("Failed to parse {} as TOML", path.display()))?;
    let Some(section_value) = value.get(section).cloned() else {
        return Ok(None);
    };
    let typed: T = section_value
        .try_into()
        .with_context(|| format!("Failed to deserialize [{section}] section"))?;
    Ok(Some(typed))
}

/// Render `value` as a document holding nothing but `[section]`.
fn rendered_section_doc<T: serde::Serialize>(section: &str, value: &T) -> Result<DocumentMut> {
    // Serialize the typed value to a toml::Value, then convert to a
    // toml_edit Item by parsing its string representation.
    let toml_value = toml::Value::try_from(value)
        .with_context(|| format!("Failed to serialize [{section}] section"))?;
    let rendered = toml::to_string_pretty(&toml::Value::Table({
        let mut t = toml::map::Map::new();
        t.insert(section.to_string(), toml_value);
        t
    }))
    .with_context(|| format!("Failed to render [{section}] section"))?;

    rendered
        .parse()
        .with_context(|| format!("Failed to re-parse rendered [{section}] section"))
}

fn write_section<T: serde::Serialize>(work_dir: &Path, section: &str, value: &T) -> Result<()> {
    let new_doc = rendered_section_doc(section, value)?;

    let section = section.to_string();
    // RMW under the directory lock so a concurrent writer (e.g. the daemon
    // plan-rename touching `[plan]`) cannot drop the section we are inserting,
    // nor we theirs.
    update_config(work_dir, |doc| {
        if let Some(item) = new_doc.get(&section) {
            doc.insert(&section, item.clone());
        } else {
            // Section serialized to nothing (empty table) — remove from doc.
            doc.remove(&section);
        }
        Ok(())
    })
}

/// Like [`write_section`], but MERGES `value`'s keys into the section instead
/// of replacing it, leaving keys no Rust struct owns exactly as they were.
///
/// `[context]` has more than one owner: [`ContextConfig`] writes the two
/// ceilings, while `prompt_cache_split` is read straight from the document by
/// `native::launch::prompt_cache_split_enabled` and belongs to no struct at
/// all. Replacing that table on a re-init would silently switch prompt cache
/// splitting back off. Single-owner sections keep using [`write_section`],
/// whose replace semantics are what they want.
fn merge_section<T: serde::Serialize>(work_dir: &Path, section: &str, value: &T) -> Result<()> {
    let new_doc = rendered_section_doc(section, value)?;

    let section = section.to_string();
    // Same RMW-under-the-directory-lock discipline as `write_section`.
    update_config(work_dir, |doc| {
        // Nothing to merge: an empty rendered section sets no keys, so it must
        // leave the existing table (and its other owners' keys) untouched.
        let Some(new_table) = new_doc.get(&section).and_then(|item| item.as_table()) else {
            return Ok(());
        };
        let existing = doc
            .entry(&section)
            .or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
        // `as_table_like_mut` covers the inline form (`context = { .. }`) too,
        // so a hand-written section keeps its other keys either way.
        match existing.as_table_like_mut() {
            Some(table) => {
                for (key, item) in new_table.iter() {
                    table.insert(key, item.clone());
                }
            }
            // Not a table at all (an operator wrote a scalar): there is nothing
            // to preserve, so replace it with the section this module owns.
            None => *existing = toml_edit::Item::Table(new_table.clone()),
        }
        Ok(())
    })
}

/// Read the persisted plan-level sandbox config (`[plan_sandbox]`).
///
/// Returns `Ok(None)` if the section is missing — callers should fall back
/// to plan-file parsing or defaults.
pub fn read_plan_sandbox(work_dir: &Path) -> Result<Option<SandboxConfig>> {
    read_section(work_dir, PLAN_SANDBOX_SECTION)
}

/// Persist the plan-level sandbox config (`[plan_sandbox]`).
pub fn write_plan_sandbox(work_dir: &Path, sandbox: &SandboxConfig) -> Result<()> {
    write_section(work_dir, PLAN_SANDBOX_SECTION, sandbox)
}

/// Read the persisted Remote Control config (`[remote_control]`).
///
/// A missing section yields `RemoteControlConfig::default()` (mode = auto),
/// so callers always get a usable value.
pub fn read_remote_control_config(work_dir: &Path) -> Result<RemoteControlConfig> {
    Ok(read_section(work_dir, REMOTE_CONTROL_SECTION)?.unwrap_or_default())
}

/// Persist the Remote Control config (`[remote_control]`).
pub fn write_remote_control_config(work_dir: &Path, config: &RemoteControlConfig) -> Result<()> {
    write_section(work_dir, REMOTE_CONTROL_SECTION, config)
}

/// Read the persisted terminal backend config (`[terminal]`).
///
/// Resolution order: a present workspace `[terminal]` section wins WHOLE; an
/// absent one falls through to `~/.loom/config.toml`'s `terminal.backend`
/// (see [`crate::user_config::UserConfig`]), then to
/// `TerminalConfig::default()` (native) when neither is set. So a missing
/// section no longer means "the default" outright — it means "the user
/// config, then the default".
pub fn read_terminal_config(work_dir: &Path) -> Result<TerminalConfig> {
    if let Some(section) = read_section::<TerminalConfig>(work_dir, TERMINAL_SECTION)? {
        return Ok(section);
    }
    Ok(TerminalConfig {
        backend: crate::user_config::UserConfig::load().terminal_backend(),
    })
}

/// Persist the terminal backend config (`[terminal]`).
pub fn write_terminal_config(work_dir: &Path, config: &TerminalConfig) -> Result<()> {
    write_section(work_dir, TERMINAL_SECTION, config)
}

/// Read the persisted context ceilings (`[context]`).
///
/// Resolution order: a present workspace `[context]` section wins WHOLE; an
/// absent one falls through to `~/.loom/config.toml`'s `context.ceiling_tokens`
/// (see [`crate::user_config::UserConfig`]) when set, then to
/// `ContextConfig::default()`. So a missing section no longer means "the
/// default" outright — it means "the user config, then the default".
///
/// This fallback is SECTION-level, not key-level, and deliberately so:
/// `[context]` deserializes through the private `ContextConfigRaw`
/// (`context_config.rs`), whose entire purpose is to tell "the TOML set this
/// key" apart from "the TOML left this to derive" *before* the built-in
/// defaults are baked in by its `From` impl — by the time
/// `read_section::<ContextConfig>` has returned, that distinction is gone, and
/// a key-level merge would silently treat a derived default as an explicit
/// setting. Only `ceiling_tokens` gets a user-level override here;
/// `subagent_ceiling_tokens` and `model_window_tokens` keep deriving from the
/// built-ins regardless of the user config, and `ContextConfig::ceiling_for`'s
/// stage-override rule is untouched.
pub fn read_context_config(work_dir: &Path) -> Result<ContextConfig> {
    if let Some(section) = read_section::<ContextConfig>(work_dir, CONTEXT_SECTION)? {
        return Ok(section);
    }
    Ok(ContextConfig::with_user_ceiling(
        crate::user_config::UserConfig::load().context_ceiling_tokens_set(),
    ))
}

/// Persist the context ceilings (`[context]`).
///
/// Merges rather than replaces: `[context]` also carries `prompt_cache_split`,
/// which no Rust struct owns (see `merge_section`).
pub fn write_context_config(work_dir: &Path, config: &ContextConfig) -> Result<()> {
    merge_section(work_dir, CONTEXT_SECTION, config)
}

/// The ceiling governing a stage's agent session, in absolute resident tokens.
///
/// ONE resolution order, and every reader of a stage ceiling must use it:
/// the stage's own `context_ceiling_tokens` -> `[context] ceiling_tokens` ->
/// `~/.loom/config.toml`'s `context.ceiling_tokens` ->
/// [`crate::models::constants::DEFAULT_CONTEXT_CEILING_TOKENS`]. Skipping a
/// middle tier makes the signal, the governor and the daemon quote different
/// numbers for one session.
///
/// Takes the stage's value rather than the `Stage` itself so `fs/` keeps no
/// dependency on the stage model — pass `stage.context_ceiling_tokens`. A
/// caller that already holds a [`ContextConfig`] uses
/// [`ContextConfig::ceiling_for`] instead, which is the same order without the
/// read.
pub fn resolve_context_ceiling_tokens(work_dir: &Path, stage_ceiling: Option<u32>) -> u32 {
    // An unreadable or unparseable config falls back to the built-in default
    // rather than failing: a ceiling is needed on every path that asks for one.
    let config = read_context_config(work_dir).unwrap_or_default();
    config.ceiling_for(stage_ceiling)
}

#[cfg(test)]
mod tests;
