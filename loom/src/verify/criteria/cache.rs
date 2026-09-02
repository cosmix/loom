//! On-disk cache of acceptance-criterion passes, keyed by command text and
//! the tree state the command ran against.
//!
//! The same acceptance command is often executed several times against an
//! unchanged worktree: once by the stage agent via `loom check`, again by
//! `loom stage complete`, again by an adjudication judge re-running a
//! disputed criterion, and again by the daemon's own verification. A pass on
//! an identical tree is the same pass — this module lets the runner skip
//! re-executing it.
//!
//! # Key
//!
//! [`compute_cache_key`] hashes the command text, the acceptance directory,
//! `git rev-parse HEAD`, the full raw output of
//! `git status --porcelain=v2 --untracked-files=all -z`, and the content of
//! every path that status lists as changed or untracked (a path status lists
//! but that no longer exists on disk contributes its name only). Any git
//! failure — no repository, an unborn `HEAD`, `git` missing — yields `None`:
//! the caller treats that as "no key" and runs the command for real.
//!
//! # Store
//!
//! Only PASSES are ever stored: a failure is always re-run, since agents fix
//! things between runs and a flaky failure must never stick. Records live at
//! `<work_dir>/acceptance-cache/<key>.json`, written atomically via
//! [`crate::fs::locking`].
//!
//! # What is never cached
//!
//! [`is_cacheable`] rejects commands that read state outside the git tree: a
//! `$NAME`/`${NAME...}` shell reference to `HOME`, `USER`, `LOGNAME`,
//! `TMPDIR`, `LOOM_HOME`, `XDG_CONFIG_HOME`, or `XDG_CACHE_HOME`, a literal
//! `~/` or `mktemp`, or a path-like token that `git check-ignore` (see the
//! sibling `cache_ignore` module) reports as ignored under the acceptance
//! directory — every one of these can pass today and fail tomorrow with no
//! tracked-tree change to invalidate the entry. A directory that is not
//! inside a git work tree is likewise never cached, since
//! [`compute_cache_key`] returns `None` for it. A changed file over 8 MiB
//! contributes its size and modified time to the key rather than its full
//! content (see [`hash_changed_paths`]).

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

use super::cache_ignore;
use crate::git::runner::{run_git, run_git_checked};

/// Trailing slice of captured output kept on a stored record (4 KiB).
const OUTPUT_TAIL_BYTES: usize = 4 * 1024;

/// Subdirectory of the loom state directory holding cache records.
const CACHE_SUBDIR: &str = "acceptance-cache";

/// Whether the acceptance runner may consult and update the on-disk pass
/// cache. `Bypass` forces every criterion to run for real, exactly as if the
/// cache did not exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CachePolicy {
    #[default]
    Use,
    Bypass,
}

impl CachePolicy {
    /// `Bypass` when the process environment has `LOOM_ACCEPTANCE_CACHE=0`;
    /// `Use` otherwise, including when the variable is unset or holds any
    /// other value. Intended to be read once, at [`super::CriteriaConfig`]
    /// construction time.
    pub fn from_env() -> Self {
        match std::env::var("LOOM_ACCEPTANCE_CACHE") {
            Ok(value) if value.trim() == "0" => CachePolicy::Bypass,
            _ => CachePolicy::Use,
        }
    }
}

/// A computed cache key: the digest used as the store filename, plus the
/// `HEAD` commit it was computed against (carried onto the stored record so
/// a cache hit can report what tree it originally passed on).
#[derive(Debug, Clone)]
pub struct CacheKey {
    pub digest: String,
    pub tree_head: String,
}

/// One stored pass. Deliberately does not keep full command output — only a
/// bounded tail, enough to show what ran without risking a multi-megabyte
/// cache file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedPass {
    pub command: String,
    pub acceptance_dir: String,
    pub tree_head: String,
    pub exit_code: i32,
    pub recorded_at: DateTime<Utc>,
    pub duration_ms: u64,
    pub stdout_tail: String,
    pub stderr_tail: String,
}

impl CachedPass {
    /// Build the record for a criterion command that just passed for real.
    pub fn from_result(
        command: &str,
        acceptance_dir: &Path,
        tree_head: &str,
        stdout: &str,
        stderr: &str,
        duration_ms: u64,
    ) -> Self {
        Self {
            command: command.to_string(),
            acceptance_dir: acceptance_dir.to_string_lossy().to_string(),
            tree_head: tree_head.to_string(),
            exit_code: 0,
            recorded_at: Utc::now(),
            duration_ms,
            stdout_tail: tail(stdout, OUTPUT_TAIL_BYTES),
            stderr_tail: tail(stderr, OUTPUT_TAIL_BYTES),
        }
    }
}

/// Literal substrings that alone mark a command ineligible for caching —
/// unlike [`FORBIDDEN_VARS`], these are not shell-variable syntax and need
/// no reference parsing.
const FORBIDDEN_LITERALS: [&str; 2] = ["~/", "mktemp"];

/// Environment variable names whose value can change between two runs
/// against an identical tree, so a command that reads one via `$NAME` or
/// `${NAME...}` shell expansion is never cacheable.
const FORBIDDEN_VARS: [&str; 7] = [
    "HOME",
    "USER",
    "LOGNAME",
    "TMPDIR",
    "LOOM_HOME",
    "XDG_CONFIG_HOME",
    "XDG_CACHE_HOME",
];

/// Commands that read state outside the git tree can pass now and fail later
/// with no tree change to invalidate the cache entry, so they are never
/// eligible for caching. See the module docs for the full list of what this
/// rejects.
pub fn is_cacheable(command: &str, acceptance_dir: &Path) -> bool {
    if FORBIDDEN_LITERALS
        .iter()
        .any(|&pattern| command.contains(pattern))
    {
        return false;
    }
    if FORBIDDEN_VARS
        .iter()
        .any(|&name| references_var(command, name))
    {
        return false;
    }
    !cache_ignore::references_ignored_path(command, acceptance_dir)
}

/// True if `command` contains a `$NAME` or `${NAME...}` shell-variable
/// reference to `name`, matched as a whole identifier (maximal munch) so
/// e.g. `$HOMEBREW_PREFIX` never matches `name = "HOME"`.
fn references_var(command: &str, name: &str) -> bool {
    let bytes = command.as_bytes();
    for (index, &byte) in bytes.iter().enumerate() {
        if byte != b'$' {
            continue;
        }
        // Safe: `$` is a single ASCII byte, so `index + 1` always lands on a
        // char boundary regardless of any multibyte text elsewhere.
        let rest = &command[index + 1..];
        let (candidate, braced) = match rest.strip_prefix('{') {
            Some(stripped) => (stripped, true),
            None => (rest, false),
        };
        let ident_len = candidate
            .bytes()
            .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_')
            .count();
        if &candidate[..ident_len] != name {
            continue;
        }
        let boundary = candidate.as_bytes().get(ident_len).copied();
        let is_reference = if braced {
            matches!(boundary, Some(b'}') | Some(b':'))
        } else {
            !matches!(boundary, Some(b) if b.is_ascii_alphanumeric() || b == b'_')
        };
        if is_reference {
            return true;
        }
    }
    false
}

/// Compute the cache key for `command` run against `acceptance_dir`, or
/// `None` if `acceptance_dir` is not inside a usable git work tree (any git
/// failure at all — no repository, no commits yet, `git` not on `PATH`).
pub fn compute_cache_key(command: &str, acceptance_dir: &Path) -> Option<CacheKey> {
    let repo_root = resolve_repo_root(acceptance_dir)?;
    let head = run_git_checked(&["rev-parse", "HEAD"], &repo_root).ok()?;
    let status = run_git(
        &["status", "--porcelain=v2", "--untracked-files=all", "-z"],
        &repo_root,
    )
    .ok()?;
    if !status.status.success() {
        return None;
    }

    let mut hasher = Sha256::new();
    hasher.update(command.as_bytes());
    hasher.update([0]);
    hasher.update(acceptance_dir.to_string_lossy().as_bytes());
    hasher.update([0]);
    hasher.update(head.as_bytes());
    hasher.update([0]);
    hasher.update(&status.stdout);
    hash_changed_paths(&mut hasher, &repo_root, &status.stdout);

    Some(CacheKey {
        digest: hex::encode(hasher.finalize()),
        tree_head: head,
    })
}

/// Above this, a changed file contributes its size and modified time to the
/// digest instead of its full content — hashing every byte of a huge build
/// artifact or log file on every acceptance run would make computing the
/// cache key itself the slow part.
const MAX_HASHED_FILE_BYTES: u64 = 8 * 1024 * 1024;

/// Fold the content of every path `git status`'s raw `-z` output lists as
/// changed or untracked into `hasher`. A regular file over
/// [`MAX_HASHED_FILE_BYTES`] contributes its size and modified time instead
/// of its content; a directory, a since-deleted path, or one this process
/// cannot read contributes its name only (already hashed above).
fn hash_changed_paths(hasher: &mut Sha256, repo_root: &Path, raw_status: &[u8]) {
    for path in extract_status_paths(raw_status) {
        hasher.update(path.as_bytes());
        hasher.update(b":");
        let full_path = repo_root.join(&path);
        let oversized_file = std::fs::metadata(&full_path)
            .ok()
            .filter(|metadata| metadata.is_file() && metadata.len() > MAX_HASHED_FILE_BYTES);
        if let Some(metadata) = oversized_file {
            hasher.update(metadata.len().to_le_bytes());
            if let Some(seconds) = mtime_seconds(&metadata) {
                hasher.update(seconds.to_le_bytes());
            }
        } else if let Ok(bytes) = std::fs::read(&full_path) {
            let mut file_hasher = Sha256::new();
            file_hasher.update(&bytes);
            hasher.update(hex::encode(file_hasher.finalize()).as_bytes());
        }
        hasher.update([0]);
    }
}

/// `metadata`'s modified time as whole seconds since the Unix epoch, or
/// `None` if the platform cannot report one.
fn mtime_seconds(metadata: &std::fs::Metadata) -> Option<u64> {
    metadata
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

/// Resolve the git work tree that contains `acceptance_dir` via
/// `git rev-parse --show-toplevel`, rather than the loom-specific
/// worktree-vs-main-repo heuristic used elsewhere: this needs the actual
/// repository git would use for a command run in `acceptance_dir`, worktree
/// or not.
fn resolve_repo_root(acceptance_dir: &Path) -> Option<PathBuf> {
    let top = run_git_checked(&["rev-parse", "--show-toplevel"], acceptance_dir).ok()?;
    Some(PathBuf::from(top))
}

/// Extract every path a `git status --porcelain=v2 -z` byte stream lists as
/// changed (staged or not) or untracked.
///
/// Understands record types `1` (ordinary) and `2` (renamed/copied — only
/// the destination path is extracted, since that is the one that can still
/// exist on disk; the origPath token that follows a `2` record in `-z` mode
/// carries no type prefix and is simply left unrecognized by the classifier
/// below) and `?` (untracked). Unmerged (`u`) and ignored (`!`, not
/// requested here) records are out of scope.
fn extract_status_paths(raw: &[u8]) -> Vec<String> {
    raw.split(|&b| b == 0)
        .filter(|token| !token.is_empty())
        .filter_map(|token| {
            let text = String::from_utf8_lossy(token);
            let skip = match text.as_bytes() {
                [b'1', b' ', ..] => 8,
                [b'2', b' ', ..] => 9,
                [b'?', b' ', ..] => 1,
                _ => return None,
            };
            skip_fields(&text, skip).map(str::to_string)
        })
        .collect()
}

/// Drop the first `n` space-separated fields of `s`, returning the
/// remainder verbatim. The remainder (always the path, the last field in
/// every record this module parses) may itself contain spaces, which is why
/// it is never split further.
fn skip_fields(s: &str, n: usize) -> Option<&str> {
    let mut rest = s;
    for _ in 0..n {
        let idx = rest.find(' ')?;
        rest = &rest[idx + 1..];
    }
    Some(rest)
}

/// Trailing slice of `s`, at most `max_bytes` long, cut on a `char`
/// boundary so a truncated tail is never invalid UTF-8.
fn tail(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut start = s.len() - max_bytes;
    while start < s.len() && !s.is_char_boundary(start) {
        start += 1;
    }
    s[start..].to_string()
}

fn cache_dir_path(work_dir: &Path) -> PathBuf {
    work_dir.join(CACHE_SUBDIR)
}

fn cache_file_path(work_dir: &Path, digest: &str) -> PathBuf {
    cache_dir_path(work_dir).join(format!("{digest}.json"))
}

/// Create `<work_dir>/acceptance-cache/` mode-0700 if it does not already
/// exist, mirroring how `WorkDir::ensure_layout` creates the other
/// `.loom/work/` state subdirectories.
fn ensure_cache_dir(work_dir: &Path) -> Result<PathBuf> {
    let dir = cache_dir_path(work_dir);
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    match builder.create(&dir) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => {
            return Err(error).with_context(|| format!("Failed to create {}", dir.display()))
        }
    }
    Ok(dir)
}

/// Look up a stored pass for `digest`. Any read or parse failure (missing
/// file, corrupt JSON, permission error) is a cache miss, not an error — a
/// damaged cache entry must never block acceptance from running for real.
pub fn lookup_pass(work_dir: &Path, digest: &str) -> Option<CachedPass> {
    let path = cache_file_path(work_dir, digest);
    let content = crate::fs::locking::locked_read(&path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Persist a fresh pass. Callers store only after a real success — see the
/// module docs for why failures are never cached.
pub fn store_pass(work_dir: &Path, digest: &str, record: &CachedPass) -> Result<()> {
    ensure_cache_dir(work_dir)?;
    let path = cache_file_path(work_dir, digest);
    let json = serde_json::to_string_pretty(record)
        .context("Failed to serialize acceptance cache record")?;
    crate::fs::locking::locked_write(&path, &json)
}
