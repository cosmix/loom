//! A stage's checkouts of base commits, kept one directory per base so a
//! criterion run at the base reuses the build an earlier run made there.
//!
//! Threat model. The stage's session, its workers and its worktree host jobs
//! run the stage's own code, and each may write its worktree and its stage
//! cache root `<cache base>/loom/stage-cache/<name>`, nothing else below the
//! cache base. Loom fills and judges a checkout with git it runs outside every
//! sandbox, so a checkout must not sit where that code can rename or link a
//! path component: a path swapped for a link toward the repository would turn
//! loom's own writes onto the operator's files. The checkouts therefore live
//! beside the stage cache, at `<cache base>/loom/stage-base/<name>/<base>`
//! ([`base_checkout_dir`]), every component made by loom, owned by this user,
//! 0700 and opened without following a link ([`ensure_base_checkouts`]). A
//! stage boundary grants none of them. The only code that writes there is a
//! confined criterion run at the base (`host_verify::child_base`), which may
//! write that one checkout and the stage cache, never the directories above.
//!
//! Such a run may still write the checkout's `.git`, whose configuration would
//! name commands (a `filter.<name>.smudge`, an `include.path`) git runs. Loom's
//! git therefore never discovers that `.git`: it runs with `GIT_DIR` naming
//! the repository's own common directory, the checkout as `GIT_WORK_TREE` and
//! an index in a private scratch directory, both to fill a new checkout and to
//! judge an existing one clean. The checkout's `.git` is a shared clone written
//! for the criteria (a test may ask git where it runs); loom writes its `HEAD`
//! and `index` files without following a link there.
//!
//! A clean checkout keeps what is ignored, the build output a warm rerun
//! wants. Only base runs wrote it, running the base commit's tracked code with
//! the stage's build caches; those caches are the stage's to poison, and they
//! reach a checkout made afresh just as they reach a reused one, so keeping
//! the output gives the stage no way in that a fresh checkout would close.
//! Revisit if a base run ever gets a cache the stage cannot write: its
//! evidence could then exclude the stage, and ignored output would need a
//! `git clean -fdx` before reuse. Checkouts of other bases are removed when
//! one is made ([`ensure_base_checkouts`]); a stage's host jobs run one at a
//! time, so none of them is in use.

use std::ffi::OsStr;
use std::fs::OpenOptions;
use std::io::{ErrorKind, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::Output;

use anyhow::{bail, ensure, Context, Result};

use super::StageCache;
use crate::git::runner::{run_git_checked, run_git_with_env};

/// The directories from the cache base down to the one holding each stage's
/// base checkouts: a sibling of the stage caches, out of every stage's reach.
pub const BASE_CHECKOUT_PARENTS: [&str; 2] = ["loom", "stage-base"];

/// `<cache base>/loom/stage-base/<stage cache root name>/<base>`, the
/// checkout of commit `base`, resolved without touching the filesystem.
pub fn base_checkout_dir(cache: &StageCache, base: &str) -> Result<PathBuf> {
    let mut dir = cache.base.clone();
    dir.extend(BASE_CHECKOUT_PARENTS);
    Ok(dir.join(cache.root_name()?).join(base))
}

/// Make the stage's directory of base checkouts as `StageCache::ensure` makes
/// its cache root, remove every checkout in it except `base`'s, and return the
/// canonical path `base`'s checkout goes to.
pub fn ensure_base_checkouts(cache: &StageCache, base: &str) -> Result<PathBuf> {
    ensure!(
        !base.is_empty() && !base.contains(['/', '\0']) && base != "." && base != "..",
        "{base:?} cannot name a base checkout"
    );
    let parent = cache.ensure_below(&BASE_CHECKOUT_PARENTS)?;
    let entries =
        std::fs::read_dir(&parent).with_context(|| format!("cannot list {}", parent.display()))?;
    for entry in entries {
        let entry = entry.with_context(|| format!("cannot list {}", parent.display()))?;
        if entry.file_name() != OsStr::new(base) {
            remove(&entry.path())?;
        }
    }
    Ok(parent.join(base))
}

/// Make `dir` a checkout of `commit` of the repository whose common git
/// directory is `common`, reusing the one there when its HEAD is `commit` and
/// its tree is clean; anything else there is deleted first. Returns whether
/// the checkout was reused. A symbolic link at `dir` or its parent is refused.
pub(crate) fn keep(dir: &Path, common: &str, commit: &str) -> Result<bool> {
    let parent = dir
        .parent()
        .context("a base checkout needs a parent directory")?;
    match std::fs::create_dir(parent) {
        Err(error) if error.kind() != ErrorKind::AlreadyExists => {
            return Err(error).with_context(|| format!("cannot create {}", parent.display()));
        }
        _ => {}
    }
    ensure!(present(parent)?, "{} vanished", parent.display());
    let scratch = tempfile::tempdir().context("cannot create a scratch directory")?;
    let index = scratch.path().join("index");
    if present(dir)? {
        if is_clean_at(dir, common, &index, commit)? {
            return Ok(true);
        }
        remove(dir)?;
    }
    std::fs::create_dir(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    fill(dir, common, scratch.path(), commit)?;
    Ok(false)
}

/// Whether anything is at `path`; a symbolic link there is refused.
fn present(path: &Path) -> Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_symlink() => bail!(
            "the base checkout path {} is a symbolic link; loom will not use it",
            path.display()
        ),
        Ok(_) => Ok(true),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).with_context(|| format!("cannot inspect {}", path.display())),
    }
}

fn remove(path: &Path) -> Result<()> {
    let removed = if std::fs::symlink_metadata(path)?.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    removed.with_context(|| format!("cannot remove the stale base checkout {}", path.display()))
}

/// A shared clone in `dir` for the criteria, filled with `commit`'s tree
/// through the repository's own git, then detached at `commit`.
fn fill(dir: &Path, common: &str, scratch: &Path, commit: &str) -> Result<()> {
    let target = dir
        .to_str()
        .context("the base checkout path is not UTF-8")?;
    let clone = [
        "clone",
        "--quiet",
        "--shared",
        "--no-checkout",
        "--",
        common,
        target,
    ];
    // Run from the scratch directory, which holds no `.git` to discover.
    run_git_checked(&clone, scratch)?;
    let index = scratch.join("index");
    checked(dir, common, &index, &["read-tree", commit])?;
    // `--index` records each file's stat data, so the criteria's git sees a clean tree.
    let populate = ["checkout-index", "--all", "--force", "--index"];
    checked(dir, common, &index, &populate)?;
    write_git_file(dir, "HEAD", format!("{commit}\n").as_bytes())?;
    let bytes = std::fs::read(&index).context("cannot read the base checkout's index")?;
    write_git_file(dir, "index", &bytes)
}

/// Whether `dir` is a checkout detached at `commit` whose tracked files match
/// it and which holds no untracked file that is not ignored.
fn is_clean_at(dir: &Path, common: &str, index: &Path, commit: &str) -> Result<bool> {
    let git = dir.join(".git");
    if !std::fs::symlink_metadata(&git).is_ok_and(|meta| meta.is_dir()) {
        return Ok(false);
    }
    let Some(head) = read_unlinked(&git.join("HEAD")) else {
        return Ok(false);
    };
    if head.trim() != commit {
        return Ok(false);
    }
    checked(dir, common, index, &["read-tree", commit])?;
    // `-q` refreshes every entry it can and reports the rest to `diff-files`.
    plumbing(dir, common, index, &["update-index", "-q", "--refresh"])?;
    if !plumbing(dir, common, index, &["diff-files", "--quiet"])?
        .status
        .success()
    {
        return Ok(false);
    }
    let others = ["ls-files", "--others", "--exclude-standard", "-z"];
    Ok(checked(dir, common, index, &others)?.stdout.is_empty())
}

/// `git <args>` on the work tree `dir` with the repository's common git
/// directory and the private `index` (see the module docs).
fn plumbing(dir: &Path, common: &str, index: &Path, args: &[&str]) -> Result<Output> {
    let env = [
        ("GIT_DIR", OsStr::new(common)),
        ("GIT_WORK_TREE", dir.as_os_str()),
        ("GIT_INDEX_FILE", index.as_os_str()),
    ];
    run_git_with_env(args, &env, dir)
}

fn checked(dir: &Path, common: &str, index: &Path, args: &[&str]) -> Result<Output> {
    let output = plumbing(dir, common, index, args)?;
    ensure!(
        output.status.success(),
        "git {} failed in the base checkout {}: {}",
        args.join(" "),
        dir.display(),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(output)
}

/// The text of `path`, `None` when it is missing, unreadable or a link.
fn read_unlinked(path: &Path) -> Option<String> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .ok()?;
    let mut text = String::new();
    file.read_to_string(&mut text).ok()?;
    Some(text)
}

/// Write `<dir>/.git/<name>`, refusing a link at `.git` or at the file.
fn write_git_file(dir: &Path, name: &str, bytes: &[u8]) -> Result<()> {
    let git = dir.join(".git");
    ensure!(
        present(&git)? && git.symlink_metadata()?.is_dir(),
        "the base checkout {} has no git directory",
        dir.display()
    );
    let path = git.join(name);
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)
        .with_context(|| format!("cannot write {}", path.display()))?;
    file.write_all(bytes)
        .with_context(|| format!("cannot write {}", path.display()))
}

#[cfg(test)]
#[path = "base_checkout_tests.rs"]
mod tests;
