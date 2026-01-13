use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::fs::knowledge::KnowledgeDir;

pub struct WorkDir {
    root: PathBuf,
}

impl WorkDir {
    pub fn new<P: AsRef<Path>>(base_path: P) -> Result<Self> {
        let root = base_path.as_ref().join(".work");
        Ok(Self { root })
    }

    pub fn initialize(&self) -> Result<()> {
        if self.root.exists() {
            bail!(".work directory already exists");
        }

        fs::create_dir_all(&self.root).context("Failed to create .work directory")?;

        let subdirs = [
            "runners",
            "tracks",
            "signals",
            "handoffs",
            "archive",
            "stages",
            "sessions",
            "logs",
            "crashes",
            "checkpoints",
            "task-state",
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
            "runners",
            "tracks",
            "signals",
            "handoffs",
            "archive",
            "stages",
            "sessions",
            "logs",
            "crashes",
            "checkpoints",
            "task-state",
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

- `runners/` - AI agent configurations and state
- `tracks/` - Conversation thread metadata
- `signals/` - Inter-agent communication
- `handoffs/` - Context handoff records
- `archive/` - Archived entities
- `stages/` - Stage definitions and status
- `sessions/` - Active session tracking
- `logs/` - Session logs and crash reports
- `knowledge/` - Curated codebase knowledge (entry points, patterns, conventions)
- `checkpoints/` - Task completion checkpoints from agents
- `task-state/` - Task progression tracking per stage

Do not manually edit these files unless you know what you're doing.
"#;

        let readme_path = self.root.join("README.md");
        fs::write(readme_path, readme_content).context("Failed to create README.md")?;

        Ok(())
    }

    pub fn runners_dir(&self) -> PathBuf {
        self.root.join("runners")
    }

    pub fn tracks_dir(&self) -> PathBuf {
        self.root.join("tracks")
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

    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }

    pub fn crashes_dir(&self) -> PathBuf {
        self.root.join("crashes")
    }

    pub fn knowledge_dir(&self) -> PathBuf {
        self.root.join("knowledge")
    }

    pub fn checkpoints_dir(&self) -> PathBuf {
        self.root.join("checkpoints")
    }

    pub fn task_state_dir(&self) -> PathBuf {
        self.root.join("task-state")
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
}
