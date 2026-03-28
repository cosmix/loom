//! Repository bootstrap helpers for Loom.
//!
//! Loom relies on git worktrees for stage isolation, which requires a
//! non-bare repository with at least one commit. These helpers ensure the
//! current project is worktree-capable without staging or committing user files.

use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::git::runner::{run_git, run_git_bool, run_git_checked};
use crate::git::worktree::check_git_available;

const INITIAL_COMMIT_MESSAGE: &str = "Initialize repository for loom";
const BOOTSTRAP_README: &str = "README.md";

/// Describes whether repository bootstrap changed the current project.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RepoBootstrapResult {
    pub initialized_repo: bool,
    pub created_initial_commit: bool,
}

impl RepoBootstrapResult {
    pub fn changed(self) -> bool {
        self.initialized_repo || self.created_initial_commit
    }
}

/// Ensure the current directory is a git repository with at least one commit.
///
/// This is the minimum git state Loom needs before it can create worktrees.
/// If the directory is not yet a git repo, this runs `git init`. If the repo
/// has no commits, it creates a bootstrap commit containing only `README.md`.
pub fn ensure_repo_ready_for_worktrees(repo_root: &Path) -> Result<RepoBootstrapResult> {
    check_git_available()?;

    let mut result = RepoBootstrapResult::default();

    if !is_git_repository(repo_root)? {
        initialize_repo(repo_root)?;
        result.initialized_repo = true;
    }

    if !has_head_commit(repo_root) {
        create_bootstrap_commit(repo_root)?;
        result.created_initial_commit = true;
    }

    Ok(result)
}

fn is_git_repository(repo_root: &Path) -> Result<bool> {
    let output = run_git(&["rev-parse", "--is-inside-work-tree"], repo_root)?;
    Ok(output.status.success())
}

fn initialize_repo(repo_root: &Path) -> Result<()> {
    run_git_checked(&["init"], repo_root).with_context(|| {
        format!(
            "Failed to initialize git repository at {}",
            repo_root.display()
        )
    })?;
    Ok(())
}

fn has_head_commit(repo_root: &Path) -> bool {
    run_git_bool(&["rev-parse", "--verify", "HEAD"], repo_root)
}

fn create_bootstrap_commit(repo_root: &Path) -> Result<()> {
    ensure_git_identity(repo_root)?;
    ensure_bootstrap_readme(repo_root)?;
    run_git_checked(&["add", "--", BOOTSTRAP_README], repo_root)
        .context("Failed to stage README.md for Loom bootstrap commit")?;
    run_git_checked(
        &[
            "commit",
            "-m",
            INITIAL_COMMIT_MESSAGE,
            "--",
            BOOTSTRAP_README,
        ],
        repo_root,
    )
    .context("Failed to create Loom bootstrap commit")?;

    Ok(())
}

fn ensure_bootstrap_readme(repo_root: &Path) -> Result<()> {
    let readme_path = repo_root.join(BOOTSTRAP_README);
    if !readme_path.exists() {
        std::fs::write(&readme_path, "")
            .context("Failed to create README.md for Loom bootstrap")?;
    }
    Ok(())
}

fn ensure_git_identity(repo_root: &Path) -> Result<()> {
    read_git_config(repo_root, "user.name")?;
    read_git_config(repo_root, "user.email")?;
    Ok(())
}

fn read_git_config(repo_root: &Path, key: &str) -> Result<String> {
    let output = run_git(&["config", "--get", key], repo_root)?;

    if output.status.success() {
        let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !value.is_empty() {
            return Ok(value);
        }
    }

    bail!(
        "Git identity is required before Loom can create its bootstrap commit. \
Set it with:\n  git config --global user.name \"Your Name\"\n  git config --global user.email \"you@example.com\""
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::runner::run_git_checked;
    use serial_test::serial;
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;
    use tempfile::TempDir;

    struct GlobalGitConfigGuard {
        previous: Option<std::ffi::OsString>,
    }

    impl GlobalGitConfigGuard {
        fn set(path: &Path) -> Self {
            let previous = env::var_os("GIT_CONFIG_GLOBAL");
            unsafe {
                env::set_var("GIT_CONFIG_GLOBAL", path);
            }
            Self { previous }
        }
    }

    impl Drop for GlobalGitConfigGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => unsafe {
                    env::set_var("GIT_CONFIG_GLOBAL", value);
                },
                None => unsafe {
                    env::remove_var("GIT_CONFIG_GLOBAL");
                },
            }
        }
    }

    fn init_repo_without_commits(path: &Path) {
        Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()
            .unwrap();
    }

    fn write_global_git_config(dir: &TempDir) -> PathBuf {
        let config_path = dir.path().join("gitconfig");

        Command::new("git")
            .args(["config", "--file"])
            .arg(&config_path)
            .args(["user.name", "Test User"])
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "--file"])
            .arg(&config_path)
            .args(["user.email", "test@example.com"])
            .output()
            .unwrap();

        config_path
    }

    #[test]
    #[serial]
    fn bootstraps_missing_repository() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = TempDir::new().unwrap();
        let config_path = write_global_git_config(&config_dir);
        let _guard = GlobalGitConfigGuard::set(&config_path);
        let repo_root = temp_dir.path();
        fs::write(repo_root.join("README.md"), "# temp\n").unwrap();

        let result = ensure_repo_ready_for_worktrees(repo_root).unwrap();

        assert_eq!(
            result,
            RepoBootstrapResult {
                initialized_repo: true,
                created_initial_commit: true,
            }
        );
        assert!(repo_root.join(".git").exists());
        assert!(
            !run_git_checked(&["rev-parse", "--verify", "HEAD"], repo_root)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            run_git_checked(&["show", "HEAD:README.md"], repo_root).unwrap(),
            "# temp"
        );

        let status = run_git_checked(&["status", "--porcelain"], repo_root).unwrap();
        assert!(status.is_empty());
    }

    #[test]
    #[serial]
    fn bootstraps_repo_without_commits() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = TempDir::new().unwrap();
        let config_path = write_global_git_config(&config_dir);
        let _guard = GlobalGitConfigGuard::set(&config_path);
        let repo_root = temp_dir.path();
        init_repo_without_commits(repo_root);

        let result = ensure_repo_ready_for_worktrees(repo_root).unwrap();

        assert_eq!(
            result,
            RepoBootstrapResult {
                initialized_repo: false,
                created_initial_commit: true,
            }
        );
        assert!(
            !run_git_checked(&["rev-parse", "--verify", "HEAD"], repo_root)
                .unwrap()
                .is_empty()
        );
        assert!(repo_root.join("README.md").exists());
        assert_eq!(fs::read_to_string(repo_root.join("README.md")).unwrap(), "");
        assert_eq!(
            run_git_checked(&["show", "HEAD:README.md"], repo_root).unwrap(),
            ""
        );
    }

    #[test]
    #[serial]
    fn preserves_staged_changes_in_unborn_repo() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = TempDir::new().unwrap();
        let config_path = write_global_git_config(&config_dir);
        let _guard = GlobalGitConfigGuard::set(&config_path);
        let repo_root = temp_dir.path();
        init_repo_without_commits(repo_root);

        fs::write(repo_root.join("tracked.txt"), "tracked\n").unwrap();
        run_git_checked(&["add", "tracked.txt"], repo_root).unwrap();

        let result = ensure_repo_ready_for_worktrees(repo_root).unwrap();

        assert!(result.created_initial_commit);
        assert!(repo_root.join("README.md").exists());

        let status = run_git_checked(&["status", "--porcelain"], repo_root).unwrap();
        assert_eq!(status, "A  tracked.txt");
    }

    #[test]
    #[serial]
    fn fails_without_git_identity() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = TempDir::new().unwrap();
        let config_path = config_dir.path().join("gitconfig");
        fs::write(&config_path, "").unwrap();
        let _guard = GlobalGitConfigGuard::set(&config_path);
        let repo_root = temp_dir.path();
        init_repo_without_commits(repo_root);

        let result = ensure_repo_ready_for_worktrees(repo_root);

        assert!(result.is_err());
        let error = result.unwrap_err().to_string();
        assert!(error.contains("Git identity is required"));
        assert!(error.contains("git config --global user.name"));
        assert!(error.contains("git config --global user.email"));
    }

    #[test]
    #[serial]
    fn noops_for_repo_with_existing_head() {
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path();
        init_repo_without_commits(repo_root);

        fs::write(repo_root.join("README.md"), "# temp\n").unwrap();
        run_git_checked(&["add", "README.md"], repo_root).unwrap();
        Command::new("git")
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-m",
                "Initial commit",
            ])
            .current_dir(repo_root)
            .output()
            .unwrap();

        let result = ensure_repo_ready_for_worktrees(repo_root).unwrap();

        assert_eq!(result, RepoBootstrapResult::default());
    }
}
