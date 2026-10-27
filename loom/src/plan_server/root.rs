//! Project root resolution.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::context::untrusted::inline_safe;
use crate::git::runner::run_git;

/// Resolve the project checkout, refusing loom stage worktrees.
pub fn resolve_project(cwd: &Path) -> Result<PathBuf> {
    let output = run_git(&["rev-parse", "--show-toplevel"], cwd)?;
    if !output.status.success() {
        bail!(
            "loom plan needs a git repository; {} is not inside one",
            safe_path(cwd)
        );
    }
    let root = git_path(output.stdout)?
        .canonicalize()
        .with_context(|| format!("cannot resolve the project root from {}", safe_path(cwd)))?;
    let branch = git_text(&["rev-parse", "--abbrev-ref", "HEAD"], &root)
        .or_else(|_| git_text(&["symbolic-ref", "--short", "HEAD"], &root))?;
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

fn git_text(args: &[&str], cwd: &Path) -> Result<String> {
    let output = run_git(args, cwd)?;
    if !output.status.success() {
        bail!("git {} failed in {}", args.join(" "), safe_path(cwd));
    }
    let text = String::from_utf8(output.stdout).context("git returned non-UTF-8 text")?;
    Ok(text.strip_suffix('\n').unwrap_or(&text).to_owned())
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
