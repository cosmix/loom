//! Resolved directories for verification commands, independent of how a
//! caller acquired the stage worktree.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// The trusted or operator path resolver supplies the root; command runners
/// use these paths without resolving a repository from their own cwd.
#[derive(Debug, Clone)]
pub(crate) struct StageExecutionPaths {
    pub worktree_root: Option<PathBuf>,
    pub acceptance_dir: Option<PathBuf>,
}

/// Resolve the declared working directory within an already chosen root.
pub(crate) fn resolve_acceptance_dir(
    worktree_root: Option<&Path>,
    working_dir: Option<&str>,
) -> Result<Option<PathBuf>> {
    match (worktree_root, working_dir) {
        (Some(root), Some(".")) | (Some(root), None) => Ok(Some(root.to_path_buf())),
        (Some(root), Some(subdir)) => {
            let full_path = root.join(subdir);
            let canonical = full_path
                .canonicalize()
                .with_context(|| missing_acceptance_dir(root, &full_path, subdir))?;
            let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
            anyhow::ensure!(
                canonical.starts_with(&canonical_root),
                "Acceptance directory {} escapes worktree root {}",
                canonical.display(),
                canonical_root.display()
            );
            Ok(Some(canonical))
        }
        _ => Ok(None),
    }
}

fn missing_acceptance_dir(root: &Path, full_path: &Path, subdir: &str) -> String {
    let mut message = format!(
        "Failed to resolve acceptance directory: {} (working_dir='{subdir}')",
        full_path.display()
    );
    for build_file in ["Cargo.toml", "package.json", "go.mod", "pyproject.toml"] {
        for candidate in [".", "loom", "app", "src", "packages"] {
            if candidate != subdir && root.join(candidate).join(build_file).exists() {
                message.push_str(&format!(
                    "\n  HINT: {build_file} found at {candidate}/ — working_dir should probably be \"{candidate}\""
                ));
            }
        }
    }
    message
}
