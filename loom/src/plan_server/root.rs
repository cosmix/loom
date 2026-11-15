//! Project root resolution.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::context::untrusted::inline_safe;
use crate::git::runner::run_git;

/// Resolve the project checkout, refusing loom stage worktrees.
pub fn resolve_project(cwd: &Path) -> Result<PathBuf> {
    let root = crate::git::revision::toplevel(cwd)
        .map_err(|cause| {
            anyhow::anyhow!(
                "loom plan needs a git repository; {} is not inside one ({})",
                safe_path(cwd),
                inline_safe(&format!("{cause:#}"))
            )
        })?
        .canonicalize()
        .with_context(|| format!("cannot resolve the project root from {}", safe_path(cwd)))?;
    let git = crate::git::worktree::WorktreeGit::discovered(&root);
    let branch = crate::git::branch::current_branch_with(&git)?;
    if branch.starts_with("loom/") {
        let common = git_path_output(
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
            &root,
        )?;
        let main = common
            .parent()
            .context("git common directory has no parent")?;
        bail!(
            "{} is a loom stage worktree (branch {}); run loom plan from the main checkout at {}",
            safe_path(&root),
            inline_safe(&branch),
            safe_path(main)
        );
    }
    Ok(root)
}

fn git_path_output(args: &[&str], cwd: &Path) -> Result<PathBuf> {
    let output = run_git(args, cwd)?;
    if !output.status.success() {
        bail!("git {} failed in {}", args.join(" "), safe_path(cwd));
    }
    git_path(output.stdout)
}

fn git_path(mut bytes: Vec<u8>) -> Result<PathBuf> {
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        Ok(PathBuf::from(std::ffi::OsString::from_vec(bytes)))
    }
    #[cfg(not(unix))]
    {
        Ok(PathBuf::from(
            String::from_utf8(bytes).context("git returned a non-UTF-8 path")?,
        ))
    }
}

fn safe_path(path: &Path) -> String {
    inline_safe(&path.to_string_lossy())
}

#[cfg(test)]
mod branch_collision_tests {
    #[test]
    fn stage_branch_with_same_named_tag_is_refused() {
        let repo = tempfile::tempdir().unwrap();
        for args in [
            vec!["init", "-q", "-b", "loom/s1"],
            vec![
                "-c",
                "user.name=T",
                "-c",
                "user.email=t@t",
                "commit",
                "--allow-empty",
                "-qm",
                "seed",
            ],
            vec!["tag", "loom/s1"],
        ] {
            let output = std::process::Command::new("git")
                .args(args)
                .current_dir(repo.path())
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
        }
        let error = super::resolve_project(repo.path()).unwrap_err().to_string();
        assert!(error.contains("loom stage worktree"), "{error}");
    }
}
