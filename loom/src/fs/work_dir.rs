use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::fs::knowledge::KnowledgeDir;

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
        let candidate = base_path.as_ref().join(".work");
        if candidate.exists() {
            return Ok(Self { root: candidate });
        }

        // Search upward for .work (like git searches for .git)
        if let Ok(abs) = base_path.as_ref().canonicalize() {
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

    pub fn initialize(&self) -> Result<()> {
        if self.root.exists() {
            bail!(".work directory already exists");
        }

        fs::create_dir_all(&self.root).context("Failed to create .work directory")?;

        let subdirs = [
            "signals", "handoffs", "archive", "stages", "sessions", "crashes",
        ];

        for subdir in &subdirs {
            let path = self.root.join(subdir);
            fs::create_dir(&path)
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
            "signals", "handoffs", "archive", "stages", "sessions", "crashes",
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
                let resolved = if link_target.is_relative() {
                    // Get parent of .work (where the symlink is located)
                    if let Some(parent) = self.root.parent() {
                        parent.join(&link_target)
                    } else {
                        return None;
                    }
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
}
