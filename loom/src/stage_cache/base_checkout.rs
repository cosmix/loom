//! A stage's checkouts of base commits, kept one directory per base so a
//! criterion run at the base reuses the build an earlier run made there, each
//! with a build cache of its own.
//!
//! Threat model. The stage's session, its workers and its worktree host jobs
//! run the stage's own code, and each may write its worktree and its stage
//! cache root `<cache base>/loom/stage-cache/<name>`, nothing else below the
//! cache base. Loom fills and judges a checkout with git it runs outside every
//! sandbox, so a checkout must not sit where that code can rename or link a
//! path component: a path swapped for a link toward the repository would turn
//! loom's own writes onto the operator's files. The checkouts therefore live
//! beside the stage cache, at `<cache base>/loom/stage-base/<name>/<base>`,
//! with the base run's cache at `<name>/cache-<base prefix>` ([`BaseDirs`]),
//! every component made by loom, owned by this user, 0700 and opened without
//! following a link ([`ensure_base_dirs`]). A stage boundary grants none of
//! them. The only code that writes there is a confined criterion run at the
//! base (`host_verify::child_base`), which may write that one checkout and its
//! base cache, never the directories above and never the stage cache.
//!
//! Such a run may still write the checkout's `.git`, whose configuration would
//! name commands (a `filter.<name>.smudge`, an `include.path`) git runs. Loom's
//! git therefore never discovers that `.git`: it runs with `GIT_DIR` naming
//! the repository's own common directory, the checkout as `GIT_WORK_TREE` and
//! an index in a private scratch directory, both to fill a new checkout and to
//! judge an existing one clean. The checkout's `.git` is a shared clone written
//! for the criteria (a test may ask git where it runs); loom writes its `HEAD`
//! and `index` files without following a link there, and reads `HEAD` only as
//! a small regular file.
//!
//! A clean checkout keeps what is ignored, the build output a warm rerun
//! wants. Only base runs at that base wrote it, running the base commit's
//! tracked code with the base cache, which no stage code can write either, so
//! the output carries nothing of the stage and keeping it is safe. The base
//! cache is keyed by the base's first [`CACHE_PREFIX`] hex digits; bases
//! sharing them would share a cache, still written only by base runs. Checkouts
//! and caches of other bases are removed when one is made
//! ([`ensure_base_dirs`]); a stage's host jobs run one at a time, so none of
//! them is in use.
//!
//! Remaining limit: a base run reads the host's Cargo home, read-only, as
//! every confined build does (`verify::criteria`): it holds the toolchains and
//! credentials and is shared by every build. A stage session whose sandbox
//! lets it write the registry sources there reaches base runs through them.
//! Revisit if confined builds get a Cargo home the stage cannot write.

use std::ffi::OsStr;
use std::fs::OpenOptions;
use std::io::{ErrorKind, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::Output;

use anyhow::{bail, ensure, Context, Result};

use super::StageCache;
use crate::fs::safe_read::read_open_bounded;
use crate::git::runner::{run_git_checked, run_git_with_env};

/// The directories from the cache base down to the one holding each stage's
/// base checkouts: a sibling of the stage caches, out of every stage's reach.
pub const BASE_CHECKOUT_PARENTS: [&str; 2] = ["loom", "stage-base"];

/// Hex digits of the base naming its cache: short, so the cache's sccache
/// socket path fits `sun_path`.
pub const CACHE_PREFIX: usize = 12;

/// Most bytes of a checkout's `.git/HEAD` loom reads: a commit id and newline.
const HEAD_MAX_BYTES: usize = 1024;

/// Where a criterion run at one base runs: its checkout and its build cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseDirs {
    /// `<cache base>/loom/stage-base/<stage cache name>/<base>`, made by `keep`.
    pub checkout: PathBuf,
    /// Rooted at `<cache base>/loom/stage-base/<stage cache name>/cache-<base
    /// prefix>`, canonical and made.
    pub cache: StageCache,
}

/// Make the stage's directory of base checkouts as `StageCache::ensure` makes
/// its cache root, remove every checkout and cache in it except `base`'s, and
/// return where `base`'s checkout goes and its cache, made.
pub fn ensure_base_dirs(cache: &StageCache, base: &str) -> Result<BaseDirs> {
    ensure!(
        !base.is_empty() && !base.contains(['/', '\0']) && base != "." && base != "..",
        "{base:?} cannot name a base checkout"
    );
    let parent = cache.ensure_below(&BASE_CHECKOUT_PARENTS)?;
    let cache_name = format!("cache-{}", base.get(..CACHE_PREFIX).unwrap_or(base));
    let entries =
        std::fs::read_dir(&parent).with_context(|| format!("cannot list {}", parent.display()))?;
    for entry in entries {
        let entry = entry.with_context(|| format!("cannot list {}", parent.display()))?;
        let name = entry.file_name();
        if name != OsStr::new(base) && name != OsStr::new(&cache_name) {
            remove(&entry.path())?;
        }
    }
    let stage_name = cache
        .root_name()?
        .to_str()
        .context("the stage cache root's name is not UTF-8")?;
    let mut parents = BASE_CHECKOUT_PARENTS.map(String::from).to_vec();
    parents.push(stage_name.to_string());
    let base_cache = StageCache {
        base: cache.base.clone(),
        parents,
        root: parent.join(&cache_name),
        socket_dir: None,
    };
    let root = base_cache.ensure()?;
    Ok(BaseDirs {
        checkout: parent.join(base),
        cache: base_cache.at_root(root),
    })
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

/// The text of `path`, `None` when it is missing, unreadable, a link, not a
/// regular file (a FIFO opens without blocking and is refused) or over
/// [`HEAD_MAX_BYTES`]: a base run may have put anything there.
fn read_unlinked(path: &Path) -> Option<String> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .ok()?;
    let bytes = read_open_bounded(file, path, HEAD_MAX_BYTES).ok()?;
    String::from_utf8(bytes).ok()
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
