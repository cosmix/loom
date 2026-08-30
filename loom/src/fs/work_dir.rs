use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::os::unix::fs::DirBuilderExt;
#[cfg(test)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use toml_edit::DocumentMut;

use crate::fs::knowledge::KnowledgeDir;
use crate::models::constants::{DEFAULT_CONTEXT_CEILING_TOKENS, DEFAULT_SUBAGENT_CEILING_TOKENS};
use crate::models::session::TerminalConfig;
use crate::plan::schema::SandboxConfig;
use crate::remote_control::RemoteControlConfig;

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
/// * `work_dir` - Path to the .work directory (not the config file itself)
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

pub struct WorkDir {
    root: PathBuf,
}

impl WorkDir {
    pub fn new<P: AsRef<Path>>(base_path: P) -> Result<Self> {
        let base = base_path.as_ref();
        let candidate = base.join(".work");
        if candidate.exists() {
            return Ok(Self { root: candidate });
        }

        // Search upward for .work (like git searches for .git)
        if let Ok(abs) = base.canonicalize() {
            let mut current = abs.as_path();
            loop {
                let work_candidate = current.join(".work");
                if work_candidate.exists() {
                    return Ok(Self {
                        root: work_candidate,
                    });
                }
                match current.parent() {
                    Some(parent) if parent != current => current = parent,
                    _ => break,
                }
            }
        }

        // Fallback: use original path (needed for initialize() which creates .work)
        Ok(Self { root: candidate })
    }

    /// Open an existing `.work/` directory or initialise it if missing.
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
            bail!(".work directory already exists");
        }

        let mut root_builder = fs::DirBuilder::new();
        root_builder.recursive(true).mode(0o700);
        root_builder
            .create(&self.root)
            .context("Failed to create .work directory")?;

        // Includes `memory`, `wrappers`, `pids` — session wrapper scripts,
        // PID tracking files, and the memory journal all live under these.
        let subdirs = [
            "signals", "handoffs", "archive", "stages", "sessions", "crashes", "memory",
            "wrappers", "pids",
        ];

        for subdir in &subdirs {
            let path = self.root.join(subdir);
            let mut builder = fs::DirBuilder::new();
            builder.mode(0o700);
            builder
                .create(&path)
                .with_context(|| format!("Failed to create {subdir} directory"))?;
        }

        self.create_readme()?;

        // Initialize knowledge directory with template files
        // KnowledgeDir expects project root (parent of .work), not work_dir
        if let Some(project_root) = self.project_root() {
            let knowledge = KnowledgeDir::new(project_root);
            knowledge.initialize()?;
        }

        Ok(())
    }

    pub fn load(&self) -> Result<()> {
        if !self.root.exists() {
            bail!(".work directory does not exist. Run 'loom init' first.");
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
                // Auto-create missing directories instead of failing
                fs::create_dir(&path)
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

    /// Path to `.work/disputes/` — adjudication artifacts. See
    /// `models/dispute.rs` for the per-id directory schema.
    pub fn disputes_dir(&self) -> PathBuf {
        self.root.join("disputes")
    }

    /// Path to `.work/plan_versions/` — plan amendment snapshots and
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
    /// * `name` - The subdirectory name relative to .work/
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

    /// Get the project root (parent of .work directory)
    pub fn project_root(&self) -> Option<&Path> {
        self.root.parent()
    }

    /// Get the main project root by following symlinks.
    ///
    /// In a worktree, `.work` is a symlink pointing to `../../.work` (the main repo's .work).
    /// This method resolves that symlink to find the true main repository root.
    ///
    /// - If `.work` is a symlink, follows it and returns the parent of the resolved path.
    /// - If `.work` is not a symlink, returns the regular project root (same as `project_root()`).
    pub fn main_project_root(&self) -> Option<PathBuf> {
        if self.root.is_symlink() {
            // Read the symlink target
            if let Ok(link_target) = fs::read_link(&self.root) {
                // If the symlink is relative, resolve it against the parent directory
                // (the parent of .work, where the symlink is located)
                let resolved = if link_target.is_relative() {
                    self.root.parent()?.join(&link_target)
                } else {
                    link_target
                };

                // Canonicalize to get the absolute path
                if let Ok(canonical) = resolved.canonicalize() {
                    // Return parent of the resolved .work directory
                    return canonical.parent().map(|p| p.to_path_buf());
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
// Centralized .work/config.toml API
//
// All read/write to `.work/config.toml` MUST go through this module so that:
//   * comments and unknown keys are preserved (toml_edit, not toml::Value),
//   * structured sub-tables (`[plan_sandbox]`) have one canonical location,
//   * concurrent access serializes through the file lock used by other
//     `fs/` writers when needed by callers.
//
// Section layout in `.work/config.toml`:
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

/// Persisted `[context]` section of `.work/config.toml`: the plan-wide context
/// ceilings, in absolute resident tokens.
///
/// Sits between a stage's own `context_ceiling_tokens` and the built-in
/// defaults, so an operator can raise or lower every stage's ceiling in one
/// place without editing the plan. Each field carries its own serde default, so
/// a half-written section still resolves to usable numbers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextConfig {
    /// Ceiling for a stage's own agent session.
    #[serde(default = "default_ceiling_tokens")]
    pub ceiling_tokens: u32,
    /// Ceiling for a subagent spawned by that session.
    #[serde(default = "default_subagent_ceiling_tokens")]
    pub subagent_ceiling_tokens: u32,
}

fn default_ceiling_tokens() -> u32 {
    DEFAULT_CONTEXT_CEILING_TOKENS
}

fn default_subagent_ceiling_tokens() -> u32 {
    DEFAULT_SUBAGENT_CEILING_TOKENS
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            ceiling_tokens: default_ceiling_tokens(),
            subagent_ceiling_tokens: default_subagent_ceiling_tokens(),
        }
    }
}

impl ContextConfig {
    /// The ceiling governing a stage's agent session, for a caller that already
    /// holds this config. The monitor reads `[context]` once per run rather than
    /// once per session per tick, so it resolves against the config in hand.
    ///
    /// This is where loom's ceiling order is defined;
    /// [`resolve_context_ceiling_tokens`] is the same order for a caller that
    /// has only a work dir.
    pub fn ceiling_for(&self, stage_ceiling: Option<u32>) -> u32 {
        stage_ceiling.unwrap_or(self.ceiling_tokens)
    }
}

fn config_path(work_dir: &Path) -> PathBuf {
    work_dir.join("config.toml")
}

/// Read `.work/config.toml` as a `toml_edit::DocumentMut`, preserving
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

/// Write the document back to `.work/config.toml`, crash-atomically and under
/// the `.work/` directory lock.
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

/// Read-modify-write `.work/config.toml` while holding the `.work/` directory
/// lock for the whole sequence.
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
/// A missing section yields `TerminalConfig::default()` (backend = native),
/// so callers always get a usable value.
pub fn read_terminal_config(work_dir: &Path) -> Result<TerminalConfig> {
    Ok(read_section(work_dir, TERMINAL_SECTION)?.unwrap_or_default())
}

/// Persist the terminal backend config (`[terminal]`).
pub fn write_terminal_config(work_dir: &Path, config: &TerminalConfig) -> Result<()> {
    write_section(work_dir, TERMINAL_SECTION, config)
}

/// Read the persisted context ceilings (`[context]`).
///
/// A missing section yields `ContextConfig::default()`, so callers always get a
/// usable ceiling without a second fallback of their own.
pub fn read_context_config(work_dir: &Path) -> Result<ContextConfig> {
    Ok(read_section(work_dir, CONTEXT_SECTION)?.unwrap_or_default())
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
/// [`DEFAULT_CONTEXT_CEILING_TOKENS`]. Skipping the middle tier makes the
/// signal, the governor and the daemon quote three different numbers for one
/// session.
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
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_main_project_root_non_symlink() {
        // Create a temporary directory structure simulating a main repo
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path();

        // Create .work directory (not a symlink)
        let work_dir_path = project_root.join(".work");
        fs::create_dir(&work_dir_path).unwrap();

        let work_dir = WorkDir::new(project_root).unwrap();

        // main_project_root should return the same as project_root for non-symlink
        let main_root = work_dir.main_project_root();
        assert!(main_root.is_some());
        assert_eq!(
            main_root.unwrap().canonicalize().unwrap(),
            project_root.canonicalize().unwrap()
        );
    }

    #[test]
    fn test_main_project_root_with_symlink() {
        // Create a temporary directory structure simulating main repo and worktree
        let temp_dir = TempDir::new().unwrap();
        let base = temp_dir.path();

        // Create main repo structure: base/main-repo/.work/
        let main_repo = base.join("main-repo");
        let main_work = main_repo.join(".work");
        fs::create_dir_all(&main_work).unwrap();

        // Create worktree structure: base/main-repo/.worktrees/my-worktree/
        let worktree = main_repo.join(".worktrees").join("my-worktree");
        fs::create_dir_all(&worktree).unwrap();

        // Create symlink: worktree/.work -> ../../.work
        let worktree_work = worktree.join(".work");
        #[cfg(unix)]
        std::os::unix::fs::symlink("../../.work", &worktree_work).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir("../../.work", &worktree_work).unwrap();

        // Create WorkDir from worktree perspective
        let work_dir = WorkDir::new(&worktree).unwrap();

        // main_project_root should follow the symlink and return main repo root
        let main_root = work_dir.main_project_root();
        assert!(main_root.is_some());
        assert_eq!(
            main_root.unwrap().canonicalize().unwrap(),
            main_repo.canonicalize().unwrap()
        );
    }

    #[test]
    fn test_main_project_root_missing_work_dir() {
        // Create a temporary directory without .work
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path();

        let work_dir = WorkDir::new(project_root).unwrap();

        // .work doesn't exist, so is_symlink() returns false
        // project_root() should still work
        let main_root = work_dir.main_project_root();
        assert!(main_root.is_some());
        assert_eq!(
            main_root.unwrap().canonicalize().unwrap(),
            project_root.canonicalize().unwrap()
        );
    }

    #[test]
    fn test_workdir_new_searches_upward() {
        let temp = TempDir::new().unwrap();
        let project_root = temp.path();

        // Create .work at project root
        let work_dir_path = project_root.join(".work");
        fs::create_dir(&work_dir_path).unwrap();

        // Create a subdirectory (simulates agent cd'ing into loom/)
        let subdir = project_root.join("loom");
        fs::create_dir(&subdir).unwrap();

        // WorkDir::new from subdirectory should find parent's .work
        let work_dir = WorkDir::new(&subdir).unwrap();
        assert_eq!(
            work_dir.root().canonicalize().unwrap(),
            work_dir_path.canonicalize().unwrap(),
            "WorkDir should find .work in parent directory"
        );
    }

    #[test]
    fn test_open_or_initialize_idempotent() {
        let temp = TempDir::new().unwrap();
        let project_root = temp.path();

        let work_dir = WorkDir::new(project_root).unwrap();
        // First call initializes
        work_dir.open_or_initialize().unwrap();
        assert!(project_root.join(".work").is_dir());

        // Second call must succeed without error
        let work_dir2 = WorkDir::new(project_root).unwrap();
        work_dir2
            .open_or_initialize()
            .expect("open_or_initialize must be idempotent on existing .work");
        // Structure still intact
        assert!(project_root.join(".work").join("stages").is_dir());
    }

    #[test]
    fn initialize_creates_private_control_directories() {
        let temp = TempDir::new().unwrap();
        let work_dir = WorkDir::new(temp.path()).unwrap();

        work_dir.initialize().unwrap();

        for path in [work_dir.root().to_path_buf(), work_dir.root().join("pids")] {
            let mode = fs::metadata(path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700);
        }
    }

    #[test]
    fn disputes_dir_path_shape() {
        let temp = TempDir::new().unwrap();
        let project_root = temp.path();
        fs::create_dir_all(project_root.join(".work")).unwrap();
        let wd = WorkDir::new(project_root).unwrap();
        assert_eq!(wd.disputes_dir(), wd.root().join("disputes"));
    }

    #[test]
    fn plan_versions_dir_path_shape() {
        let temp = TempDir::new().unwrap();
        let project_root = temp.path();
        fs::create_dir_all(project_root.join(".work")).unwrap();
        let wd = WorkDir::new(project_root).unwrap();
        assert_eq!(wd.plan_versions_dir(), wd.root().join("plan_versions"));
    }

    #[test]
    fn test_workdir_new_falls_back_when_no_work_dir() {
        let temp = TempDir::new().unwrap();
        let project_root = temp.path();

        // No .work anywhere
        let subdir = project_root.join("some/nested/dir");
        fs::create_dir_all(&subdir).unwrap();

        // WorkDir::new should fall back to subdir/.work
        let work_dir = WorkDir::new(&subdir).unwrap();
        assert_eq!(
            work_dir.root(),
            subdir.join(".work"),
            "Without .work anywhere, should fall back to base_path/.work"
        );
    }

    // ----- Centralized config.toml API tests -----

    use crate::plan::schema::SandboxConfig;

    fn init_work(temp: &TempDir) -> PathBuf {
        let work = temp.path().join(".work");
        fs::create_dir_all(&work).unwrap();
        work
    }

    #[test]
    fn read_config_returns_empty_when_missing() {
        let temp = TempDir::new().unwrap();
        let work = init_work(&temp);
        let doc = read_config(&work).unwrap();
        assert!(doc.iter().next().is_none());
    }

    #[test]
    fn read_config_preserves_comments_and_unknown_keys() {
        let temp = TempDir::new().unwrap();
        let work = init_work(&temp);
        let original = "# Top comment\n\n[plan]\n# inner comment\nsource_path = \"docs/plans/PLAN-x.md\"\nplan_id = \"x\"\nplan_name = \"X\"\nbase_branch = \"main\"\nunknown_key = \"keep me\"\n";
        fs::write(work.join("config.toml"), original).unwrap();

        let doc = read_config(&work).unwrap();
        write_config(&work, &doc).unwrap();
        let after = fs::read_to_string(work.join("config.toml")).unwrap();
        assert!(after.contains("# Top comment"));
        assert!(after.contains("# inner comment"));
        assert!(after.contains("unknown_key = \"keep me\""));
    }

    #[test]
    fn write_then_read_plan_sandbox_round_trip() {
        let temp = TempDir::new().unwrap();
        let work = init_work(&temp);
        let mut sandbox = SandboxConfig::default();
        sandbox.network.allowed_domains = vec!["github.com".to_string()];
        write_plan_sandbox(&work, &sandbox).unwrap();
        let back = read_plan_sandbox(&work).unwrap().unwrap();
        assert_eq!(back.network.allowed_domains, vec!["github.com".to_string()]);
        assert_eq!(back.enabled, sandbox.enabled);
    }

    #[test]
    fn writes_preserve_unrelated_sections_and_comments() {
        let temp = TempDir::new().unwrap();
        let work = init_work(&temp);
        let original = "# Header\n[plan]\nsource_path = \"a\"\nplan_id = \"id\"\nplan_name = \"n\"\nbase_branch = \"main\"\n# trailing comment\n";
        fs::write(work.join("config.toml"), original).unwrap();

        let mut sandbox = SandboxConfig::default();
        sandbox.network.allowed_domains = vec!["github.com".to_string()];
        write_plan_sandbox(&work, &sandbox).unwrap();

        let after = fs::read_to_string(work.join("config.toml")).unwrap();
        assert!(after.contains("[plan]"));
        assert!(after.contains("source_path = \"a\""));
        assert!(after.contains("# Header"));
        assert!(after.contains("[plan_sandbox]"));
    }

    #[test]
    fn write_then_read_terminal_config_round_trip() {
        let temp = TempDir::new().unwrap();
        let work = init_work(&temp);
        let config = TerminalConfig {
            backend: crate::models::session::SessionBackendKind::Tmux,
        };
        write_terminal_config(&work, &config).unwrap();
        let back = read_terminal_config(&work).unwrap();
        assert_eq!(back, config);
    }

    #[test]
    fn read_terminal_config_defaults_when_section_missing() {
        let temp = TempDir::new().unwrap();
        let work = init_work(&temp);
        let config = read_terminal_config(&work).unwrap();
        assert_eq!(config, TerminalConfig::default());
        assert_eq!(
            config.backend,
            crate::models::session::SessionBackendKind::Native
        );
    }

    #[test]
    fn read_returns_none_when_section_absent() {
        let temp = TempDir::new().unwrap();
        let work = init_work(&temp);
        fs::write(
            work.join("config.toml"),
            "[plan]\nsource_path = \"x\"\nplan_id = \"id\"\nplan_name = \"n\"\nbase_branch = \"main\"\n",
        )
        .unwrap();
        assert!(read_plan_sandbox(&work).unwrap().is_none());
    }

    #[test]
    fn update_config_round_trips_under_lock() {
        let temp = TempDir::new().unwrap();
        let work = init_work(&temp);
        update_config(&work, |doc| {
            let plan = doc.entry("plan").or_insert(toml_edit::table());
            if let Some(t) = plan.as_table_mut() {
                t["plan_id"] = toml_edit::value("abc");
            }
            Ok(())
        })
        .unwrap();
        let doc = read_config(&work).unwrap();
        assert_eq!(doc["plan"]["plan_id"].as_str(), Some("abc"));
        // No stray temp file left behind by the atomic write.
        assert!(!work.join("config.toml.tmp").exists());
    }

    #[test]
    fn write_section_preserves_concurrently_written_plan_section() {
        // Simulates the daemon plan-rename ([plan]) and a CLI section write
        // ([plan_sandbox]) not clobbering each other. Sequential here, but the
        // point is that write_section reads-modifies-writes under the lock and
        // therefore preserves the pre-existing [plan] section.
        let temp = TempDir::new().unwrap();
        let work = init_work(&temp);

        // Daemon writes the plan section first.
        update_config(&work, |doc| {
            let plan = doc.entry("plan").or_insert(toml_edit::table());
            if let Some(t) = plan.as_table_mut() {
                t["source_path"] = toml_edit::value("doc/plans/PLAN-x.md");
                t["plan_id"] = toml_edit::value("x");
            }
            Ok(())
        })
        .unwrap();

        // CLI writes a different section.
        let mut sandbox = SandboxConfig::default();
        sandbox.network.allowed_domains = vec!["github.com".to_string()];
        write_plan_sandbox(&work, &sandbox).unwrap();

        // Both sections survive.
        let doc = read_config(&work).unwrap();
        assert_eq!(
            doc["plan"]["source_path"].as_str(),
            Some("doc/plans/PLAN-x.md")
        );
        assert!(doc.get("plan_sandbox").is_some());
    }

    /// Regression: a stale `[project_execution]` table left over from the
    /// removed multi-backend scaffolding must not break config reads. The
    /// table is an unknown section now — `toml_edit` round-trips it
    /// harmlessly and the normal read path is unaffected.
    #[test]
    fn stale_project_execution_section_is_harmless() {
        let temp = TempDir::new().unwrap();
        let work = init_work(&temp);
        let original = "[plan]\nsource_path = \"x\"\nplan_id = \"id\"\nplan_name = \"n\"\nbase_branch = \"main\"\n\n[project_execution]\nbackend = \"native\"\n";
        fs::write(work.join("config.toml"), original).unwrap();

        // Normal config read path succeeds despite the stale table.
        let doc = read_config(&work).unwrap();
        assert!(doc.get("plan").is_some());

        // The same path used by other section readers also succeeds and the
        // stale table has no runtime effect (no known section consumes it).
        assert!(read_plan_sandbox(&work).unwrap().is_none());

        // Writing an unrelated section preserves the stale table verbatim —
        // harmless, no behavior change.
        let sandbox = SandboxConfig::default();
        write_plan_sandbox(&work, &sandbox).unwrap();
        let after = fs::read_to_string(work.join("config.toml")).unwrap();
        assert!(after.contains("[project_execution]"));
        assert!(after.contains("[plan_sandbox]"));
    }

    /// Regression: `[context]` has a second owner. `prompt_cache_split` is read
    /// straight from the document by `native::launch::prompt_cache_split_enabled`
    /// and belongs to no struct, so a re-init writing the ceilings must not take
    /// it out — that would silently switch prompt cache splitting back off.
    #[test]
    fn write_context_config_preserves_prompt_cache_split() {
        let temp = TempDir::new().unwrap();
        let work = init_work(&temp);

        update_config(&work, |doc| {
            let context = doc.entry(CONTEXT_SECTION).or_insert(toml_edit::table());
            if let Some(table) = context.as_table_mut() {
                table["prompt_cache_split"] = toml_edit::value(true);
                table["ceiling_tokens"] = toml_edit::value(90_000_i64);
            }
            Ok(())
        })
        .unwrap();

        write_context_config(
            &work,
            &ContextConfig {
                ceiling_tokens: 200_000,
                subagent_ceiling_tokens: 100_000,
            },
        )
        .unwrap();

        let doc = read_config(&work).unwrap();
        assert_eq!(
            doc[CONTEXT_SECTION]["prompt_cache_split"].as_bool(),
            Some(true)
        );
        // The keys ContextConfig owns are still overwritten.
        let config = read_context_config(&work).unwrap();
        assert_eq!(config.ceiling_tokens, 200_000);
        assert_eq!(config.subagent_ceiling_tokens, 100_000);
    }

    #[test]
    fn resolve_context_ceiling_walks_stage_then_config_then_default() {
        let temp = TempDir::new().unwrap();
        let work = init_work(&temp);

        // No config: the built-in default.
        assert_eq!(
            resolve_context_ceiling_tokens(&work, None),
            DEFAULT_CONTEXT_CEILING_TOKENS
        );

        write_context_config(
            &work,
            &ContextConfig {
                ceiling_tokens: 250_000,
                subagent_ceiling_tokens: 100_000,
            },
        )
        .unwrap();

        // Config tier wins over the default, stage tier over both.
        assert_eq!(resolve_context_ceiling_tokens(&work, None), 250_000);
        assert_eq!(resolve_context_ceiling_tokens(&work, Some(80_000)), 80_000);
    }
}
