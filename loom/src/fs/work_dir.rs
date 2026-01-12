use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

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
            "runners", "tracks", "signals", "handoffs", "archive", "stages", "sessions", "logs",
            "crashes", "checkpoints", "task-state",
        ];

        for subdir in &subdirs {
            let path = self.root.join(subdir);
            fs::create_dir(&path)
                .with_context(|| format!("Failed to create {subdir} directory"))?;
        }

        self.create_readme()?;

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
            "runners", "tracks", "signals", "handoffs", "archive", "stages", "sessions", "logs",
            "crashes", "checkpoints", "task-state",
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
- `logs/` - Tmux session logs and crash reports
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
}
