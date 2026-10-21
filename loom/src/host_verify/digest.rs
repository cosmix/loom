//! The tree digest a session and the daemon both compute, so a host job runs
//! on exactly the tree the session saw. It hashes `git status` entries and the
//! current content of each changed path; a committed clean tree hashes the
//! empty input.
//!
//! A session reads its own checkout as git discovers it. A process outside
//! the stage's sandbox (the host child, the daemon applying a freeze) reads
//! through `WorktreeGit::pinned` instead (the `*_in` functions), so the
//! worktree's `.git` file cannot name a git directory whose configuration
//! runs a command. The command, its environment and the hashing are
//! otherwise the same, so both sides compute the same digest for one tree.

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use anyhow::{bail, ensure, Context, Result};
use sha2::{Digest, Sha256};

use crate::fs::safe_read::read_bounded;
use crate::git::target_guard::knowledge_prefix;
use crate::git::worktree::WorktreeGit;
use crate::handoff::completion::content::completion_scaffold;

/// Largest file the digest will read; a larger one is an error, not a skip.
const MAX_FILE_BYTES: usize = 64 * 1024 * 1024;

/// What every digest read adds to git's environment, so a session and the
/// daemon read the same ignore rules: no optional locks (as
/// `--no-optional-locks`), no user or system config, and an
/// `XDG_CONFIG_HOME` that cannot exist (`/dev/null` is no directory). Git
/// reads its default excludes and attributes files,
/// `$XDG_CONFIG_HOME/git/ignore` and `git/attributes` (or under
/// `$HOME/.config` when the variable is unset), whatever the global config
/// says, and the session's sandbox and the daemon see different `HOME`s.
const DIGEST_ENV: [(&str, &str); 4] = [
    ("GIT_OPTIONAL_LOCKS", "0"),
    ("GIT_CONFIG_GLOBAL", "/dev/null"),
    ("GIT_CONFIG_NOSYSTEM", "1"),
    ("XDG_CONFIG_HOME", "/dev/null/loom-no-xdg-config"),
];

/// Digest of the whole checkout, read as git discovers it.
pub fn tree_digest(checkout: &Path) -> Result<String> {
    tree_digest_in(&WorktreeGit::discovered(checkout))
}

/// Digest scoped to the knowledge directory of the repository at `repo_root`.
pub fn knowledge_tree_digest(repo_root: &Path) -> Result<String> {
    knowledge_tree_digest_in(&WorktreeGit::discovered(repo_root))
}

/// The commit `checkout`'s HEAD names.
pub fn head(checkout: &Path) -> Result<String> {
    head_in(&WorktreeGit::discovered(checkout))
}

/// [`tree_digest`] of `git`'s worktree, with git run through `git`.
pub fn tree_digest_in(git: &WorktreeGit) -> Result<String> {
    digest(git, None)
}

/// [`knowledge_tree_digest`] of `git`'s worktree, with git run through `git`.
pub fn knowledge_tree_digest_in(git: &WorktreeGit) -> Result<String> {
    digest(git, Some(knowledge_prefix()))
}

/// [`head`] of `git`'s worktree, with git run through `git`.
pub fn head_in(git: &WorktreeGit) -> Result<String> {
    let out = read(git, &["rev-parse", "--verify", "HEAD"])?;
    Ok(String::from_utf8_lossy(&out).trim().to_string())
}

fn digest(git: &WorktreeGit, scope: Option<&str>) -> Result<String> {
    // Resolved, because a file is read without following a symlink at any
    // component below it (`hash_file`).
    let checkout = git
        .work_tree()
        .canonicalize()
        .with_context(|| format!("cannot resolve {}", git.work_tree().display()))?;
    let status = porcelain(git, scope)?;
    let mut hasher = Sha256::new();
    for record in entries(&status)? {
        if record.untracked && completion_scaffold(&record.path, &checkout) {
            continue;
        }
        hasher.update(record.raw);
        hasher.update([0]);
        hash_content(&mut hasher, &checkout, &record.path)?;
    }
    Ok(hex::encode(hasher.finalize()))
}

/// The stdout of a successful `git <args>` through `git` with
/// [`DIGEST_ENV`], bounded by the runner's read deadline.
fn read(git: &WorktreeGit, args: &[&str]) -> Result<Vec<u8>> {
    let checkout = git.work_tree();
    let env = DIGEST_ENV.map(|(key, value)| (key, OsStr::new(value)));
    let output = git
        .read_isolated(args, &env)
        .with_context(|| format!("failed to run git in {}", checkout.display()))?;
    ensure!(
        output.status.success(),
        "git failed in {}: {}",
        checkout.display(),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(output.stdout)
}

fn porcelain(git: &WorktreeGit, scope: Option<&str>) -> Result<Vec<u8>> {
    let mut args = vec![
        "status",
        "--porcelain=v2",
        "-z",
        "--untracked-files=all",
        "--no-renames",
    ];
    if let Some(prefix) = scope {
        args.extend(["--", prefix]);
    }
    read(git, &args)
}

struct Entry<'a> {
    raw: &'a [u8],
    path: String,
    untracked: bool,
}

/// Split the `-z` output into entries. Ordinary (`1`) and unmerged (`u`)
/// entries carry the path after a fixed number of space-separated fields;
/// `--no-renames` leaves no `2` entry.
fn entries(status: &[u8]) -> Result<Vec<Entry<'_>>> {
    let mut parsed = Vec::new();
    for raw in status
        .split(|byte| *byte == 0)
        .filter(|raw| !raw.is_empty())
    {
        let (fields, untracked) = match raw[0] {
            b'1' => (9, false),
            b'u' => (11, false),
            b'?' => (2, true),
            other => bail!("unexpected git status entry type '{}'", other as char),
        };
        let path = raw
            .splitn(fields, |byte| *byte == b' ')
            .nth(fields - 1)
            .context("malformed git status entry")?;
        let path = std::str::from_utf8(path).context("git status path is not UTF-8")?;
        parsed.push(Entry {
            raw,
            path: path.to_string(),
            untracked,
        });
    }
    Ok(parsed)
}

/// One type byte and the content framed by an 8-byte big-endian length:
/// `F` a regular file, `L` a symlink's target, `-` a path that is absent or
/// of another type.
fn hash_content(hasher: &mut Sha256, checkout: &Path, relative: &str) -> Result<()> {
    let path = checkout.join(relative);
    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            absent(hasher);
            return Ok(());
        }
        Err(error) => return Err(error).with_context(|| format!("cannot stat {}", path.display())),
    };
    let kind = metadata.file_type();
    if kind.is_symlink() {
        let target = std::fs::read_link(&path)
            .with_context(|| format!("cannot read link {}", path.display()))?;
        let bytes = target.as_os_str().as_bytes();
        hasher.update(b"L");
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(bytes);
    } else if kind.is_file() {
        ensure!(
            metadata.len() <= MAX_FILE_BYTES as u64,
            "{} is {} bytes, over the {MAX_FILE_BYTES}-byte limit for a tree digest",
            path.display(),
            metadata.len()
        );
        hash_file(hasher, checkout, relative)?;
    } else {
        absent(hasher);
    }
    Ok(())
}

fn absent(hasher: &mut Sha256) {
    hasher.update(b"-");
    hasher.update([0u8; 8]);
}

/// Hash the regular file `relative` below `checkout`, at most
/// [`MAX_FILE_BYTES`] of it. The daemon computes this digest too, so the file
/// is opened without following a symlink at any component and without
/// blocking: a FIFO or symlink swapped in after the stat is an error, never a
/// stalled tick or a read outside the checkout.
fn hash_file(hasher: &mut Sha256, checkout: &Path, relative: &str) -> Result<()> {
    let bytes = read_bounded(checkout, Path::new(relative), MAX_FILE_BYTES).with_context(|| {
        format!(
            "cannot read {} for a tree digest",
            checkout.join(relative).display()
        )
    })?;
    hasher.update(b"F");
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(&bytes);
    Ok(())
}
