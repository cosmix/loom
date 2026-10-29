//! Detect configuration edits made while Codex reviews a plan.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Context, Result};
use nix::errno::Errno;
use nix::fcntl::{readlinkat, AtFlags};
use nix::sys::stat::fstatat;
use sha2::{Digest, Sha256};

use crate::fs::safe_fs::{open_safely, safe_open_dirfd};
use crate::fs::safe_read::{kind_of_mode, list_dir_no_follow, read_bounded, EntryKind};

// Fail closed above this per-file limit; hashing a prefix would hide edits.
const MAX_CONFIG_BYTES: usize = 4 * 1024 * 1024;

/// Links followed while resolving one watched symlink, Linux's own limit; a longer chain, a
/// loop included, is recorded like a dangling link.
const MAX_LINK_HOPS: usize = 40;

/// Entries one snapshot visits at most. Links that re-serve an ancestor directory multiply the
/// tree (each ancestor re-walks its subtree under a longer path), so past this the snapshot
/// fails closed. Far above any real `.claude/`; revisit if one legitimately nears it.
const MAX_ENTRIES: usize = 100_000;

/// Linux `PATH_MAX`: Claude Code cannot open a longer configuration path, and a link chain
/// that grows the shown path without bound fails closed here instead of exhausting memory.
const MAX_PATH_BYTES: usize = 4096;

/// Repository state directories that change on every run; walking them blocks every run.
const STATE_DIRS: [&str; 2] = [".git", ".loom"];

/// The directory the walk starts from, as shown in snapshot paths.
const CLAUDE_DIR: &str = ".claude";

/// Claude Code's own worktree checkouts (`claude --worktree`) live in `.claude/worktrees/`.
/// Each is a full repository checkout, not configuration for the sessions loom starts at the
/// repository root, so a build tree or a concurrent session there must neither fail the
/// snapshot nor end the run. Revisit if Claude Code ever reads configuration from there for
/// root sessions.
const CLAUDE_WORKTREES: &str = "worktrees";

/// One fingerprint per sorted path as Claude Code reads it: the SHA-256 of a regular file, the
/// target of a symlink plus what it serves inside the repository, or the kind of any other
/// non-directory entry.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigSnapshot {
    files: BTreeMap<String, String>,
}

/// Snapshot everything in `.claude/`, plus root `.mcp.json` and `CLAUDE.md`.
///
/// No entry is opened through a symlink. A symlink is recorded by its own target (Claude Code
/// follows it, so adding, removing or retargeting one is a change) and, when it resolves inside
/// `root`, by what it serves: a regular file's content, the kind of anything else and, for a
/// link at or inside `.claude/`, a directory's tree under the link's own path (walked like
/// `.claude/`; root `.mcp.json` and `CLAUDE.md` are read as files, so a directory they serve
/// is recorded by kind). A dangling link, a loop, or a target outside `root` keeps the target
/// alone: outside the repository is not watched. A directory tree already fingerprinted
/// (`.claude/` itself, or one a link reached first) is not walked again. A FIFO, socket or
/// device is recorded by its kind. Fails closed on unsafe races, read errors, a link serving
/// the repository root, `.git` or `.loom` (changed by every run), more than `MAX_ENTRIES`
/// entries, or a path longer than `MAX_PATH_BYTES`.
///
/// `.claude/worktrees/` is skipped when it is a real directory (see `CLAUDE_WORKTREES`). When it
/// is a symlink, or another link serves a directory under it, the link is configuration a
/// session could read and is handled like any other link.
pub fn snapshot(root: &Path) -> Result<ConfigSnapshot> {
    let mut walker = Walker::new(root)?;
    for (name, kind) in listing(&walker.anchor, Path::new(""))? {
        let relative = PathBuf::from(name);
        match (relative.to_str(), kind) {
            (Some(".claude"), EntryKind::Directory) => {
                walker.schedule(&relative, relative.clone())?
            }
            (Some(".claude" | ".mcp.json" | "CLAUDE.md"), _) => {
                walker.record(&relative, &relative, kind)?;
            }
            _ => {}
        }
    }
    walker.walk()?;
    Ok(ConfigSnapshot {
        files: walker.files,
    })
}

/// Return added, removed, and modified paths in repository-relative order.
pub fn changed(before: &ConfigSnapshot, after: &ConfigSnapshot) -> Vec<String> {
    before
        .files
        .keys()
        .chain(after.files.keys())
        .filter(|path| before.files.get(*path) != after.files.get(*path))
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

struct Walker<'a> {
    root: &'a Path,
    /// `root` as given and canonicalized: an absolute link target under either is inside.
    spellings: Vec<PathBuf>,
    anchor: OwnedFd,
    /// Real directories whose trees are fingerprinted or queued; a link into one adds no walk.
    walked: Vec<PathBuf>,
    /// Real directories left out of a walked tree (`CLAUDE_WORKTREES`); a link into one still
    /// needs its own walk.
    skipped: Vec<PathBuf>,
    /// Directories still to list, as (path shown, real directory): one explicit stack, so a
    /// long link chain costs heap, never call depth.
    pending: Vec<(PathBuf, PathBuf)>,
    /// Entries visited so far, against `MAX_ENTRIES`.
    visited: usize,
    /// SHA-256 per real file already read, so many links serving one file read it once.
    digests: BTreeMap<PathBuf, String>,
    files: BTreeMap<String, String>,
}

impl<'a> Walker<'a> {
    fn new(root: &'a Path) -> Result<Self> {
        let anchor = safe_open_dirfd(root)?;
        let mut spellings = vec![root.to_path_buf()];
        let canonical = std::fs::canonicalize(root).ok();
        spellings.extend(canonical.filter(|canonical| canonical != root));
        Ok(Walker {
            root,
            spellings,
            anchor,
            walked: Vec::new(),
            skipped: Vec::new(),
            pending: Vec::new(),
            visited: 0,
            digests: BTreeMap::new(),
            files: BTreeMap::new(),
        })
    }

    /// Queue the real directory `served` to be walked under the path `shown`, unless a tree
    /// already walked or queued covers it.
    fn schedule(&mut self, shown: &Path, served: PathBuf) -> Result<()> {
        if let Some(what) = unwatchable(&served) {
            bail!(
                "{} serves {what}; session configuration cannot be checked",
                shown.display()
            );
        }
        if !self.covered(&served) {
            self.walked.push(served.clone());
            self.pending.push((shown.to_path_buf(), served));
        }
        Ok(())
    }

    /// Whether a walked or queued tree already fingerprints `served`: one that holds it and
    /// does not leave it out as a skipped directory (a tree walked from inside the skipped
    /// directory holds what is under it).
    fn covered(&self, served: &Path) -> bool {
        self.walked.iter().any(|done| {
            served.starts_with(done)
                && !self
                    .skipped
                    .iter()
                    .any(|left_out| served.starts_with(left_out) && !done.starts_with(left_out))
        })
    }

    /// Fingerprint every queued tree, including those the links it meets queue.
    fn walk(&mut self) -> Result<()> {
        while let Some((shown, real)) = self.pending.pop() {
            let mut entries = listing(&self.anchor, &real)
                .with_context(|| format!("Cannot snapshot {}", shown.display()))?;
            self.leave_out_worktrees(&shown, &real, &mut entries);
            for (name, kind) in entries {
                let (shown, real) = (shown.join(&name), real.join(&name));
                self.admit(&shown)?;
                match kind {
                    EntryKind::Directory => self.pending.push((shown, real)),
                    _ => self.record(&shown, &real, kind)?,
                }
            }
        }
        Ok(())
    }

    /// Remove `CLAUDE_WORKTREES` from the listing of the walked `.claude/` when it is a real
    /// directory, before any entry is recorded, so a link met earlier in the listing already
    /// sees it as left out. A symlink of that name is not a `Directory` here and is kept.
    fn leave_out_worktrees(
        &mut self,
        shown: &Path,
        real: &Path,
        entries: &mut Vec<(OsString, EntryKind)>,
    ) {
        if shown != Path::new(CLAUDE_DIR) {
            return;
        }
        let worktrees = entries
            .iter()
            .position(|(name, kind)| name == CLAUDE_WORKTREES && *kind == EntryKind::Directory);
        if let Some(at) = worktrees {
            entries.remove(at);
            self.skipped.push(real.join(CLAUDE_WORKTREES));
        }
    }

    /// Count one visited entry, failing closed past the entry and path budgets.
    fn admit(&mut self, shown: &Path) -> Result<()> {
        self.visited += 1;
        if self.visited > MAX_ENTRIES {
            bail!(
                "more than {MAX_ENTRIES} configuration entries (symlinks re-serving directories?); \
                 session configuration cannot be checked"
            );
        }
        if shown.as_os_str().len() > MAX_PATH_BYTES {
            let top: PathBuf = shown.iter().take(3).collect();
            bail!(
                "a configuration path under {} is longer than {MAX_PATH_BYTES} bytes; session \
                 configuration cannot be checked",
                top.display()
            );
        }
        Ok(())
    }

    /// Fingerprint one entry; a directory outside `.claude/` (a `CLAUDE.md/`) is recorded by kind.
    fn record(&mut self, shown: &Path, real: &Path, kind: EntryKind) -> Result<()> {
        let path = shown
            .to_str()
            .with_context(|| format!("Configuration path is not UTF-8: {}", shown.display()))?
            .to_owned();
        let fingerprint = match kind {
            EntryKind::File => self.digest(real),
            EntryKind::Symlink => self.link(shown, real),
            EntryKind::Special => Ok("special".to_owned()),
            EntryKind::Directory => Ok("directory".to_owned()),
        }
        .with_context(|| format!("Cannot snapshot {path}"))?;
        self.files.insert(path, fingerprint);
        Ok(())
    }

    fn digest(&mut self, real: &Path) -> Result<String> {
        if let Some(known) = self.digests.get(real) {
            return Ok(known.clone());
        }
        let bytes = read_bounded(self.root, real, MAX_CONFIG_BYTES)?;
        let digest = hex::encode(Sha256::digest(&bytes));
        self.digests.insert(real.to_path_buf(), digest.clone());
        Ok(digest)
    }

    /// A symlink's own target, then what it serves when it resolves inside the repository; a
    /// directory served at or inside `.claude/` is queued for walking.
    fn link(&mut self, shown: &Path, real: &Path) -> Result<String> {
        let target = at_parent(&self.anchor, real, |dir, name| readlinkat(dir, name))?;
        let mut fingerprint = format!("symlink:{}", hex::encode(target.as_bytes()));
        match self.resolve(real)? {
            None => {}
            Some((served, EntryKind::File)) => {
                fingerprint.push_str(&format!(":file:{}", self.digest(&served)?));
            }
            Some((served, EntryKind::Directory)) => {
                let at = hex::encode(served.as_os_str().as_bytes());
                fingerprint.push_str(&format!(":directory:{at}"));
                if shown.starts_with(".claude") {
                    self.schedule(shown, served)?;
                }
            }
            Some(_) => fingerprint.push_str(":special"),
        }
        Ok(fingerprint)
    }

    /// The repository-relative path the symlink `link` finally serves, and its kind. Resolved
    /// one component at a time from the anchor, each read without following it, so `..` after
    /// a real directory is its real parent. `None` for a dangling link, a component that is
    /// not a directory, a loop or over-long chain, or a target outside the repository.
    fn resolve(&self, link: &Path) -> Result<Option<(PathBuf, EntryKind)>> {
        let mut resolved = PathBuf::new();
        let mut pending = Vec::new();
        push_components(&mut pending, link);
        let mut hops = 0;
        while let Some(part) = pending.pop() {
            if part == ".." {
                if !resolved.pop() {
                    return Ok(None);
                }
                continue;
            }
            let candidate = resolved.join(&part);
            match kind_at(&self.anchor, &candidate)? {
                None => return Ok(None),
                Some(EntryKind::Directory) => resolved = candidate,
                Some(EntryKind::Symlink) => {
                    hops += 1;
                    if hops > MAX_LINK_HOPS {
                        return Ok(None);
                    }
                    let target =
                        at_parent(&self.anchor, &candidate, |dir, name| readlinkat(dir, name))?;
                    let target = PathBuf::from(target);
                    if target.is_absolute() {
                        let Some(inside) = self.inside(&target) else {
                            return Ok(None);
                        };
                        resolved = PathBuf::new();
                        push_components(&mut pending, &inside);
                    } else {
                        push_components(&mut pending, &target);
                    }
                }
                Some(kind) if pending.is_empty() => return Ok(Some((candidate, kind))),
                Some(_) => return Ok(None),
            }
        }
        Ok(Some((resolved, EntryKind::Directory)))
    }

    /// An absolute target's path relative to the repository root, lexically.
    fn inside(&self, target: &Path) -> Option<PathBuf> {
        let mut relative = self.spellings.iter().map(|root| target.strip_prefix(root));
        relative.find_map(Result::ok).map(Path::to_path_buf)
    }
}

/// What a served directory is when walking it would fingerprint state every run changes: the
/// repository root, or `.git`/`.loom` or a directory inside one.
fn unwatchable(served: &Path) -> Option<String> {
    if served.as_os_str().is_empty() {
        return Some("the repository root".to_owned());
    }
    STATE_DIRS
        .iter()
        .find(|state| served.starts_with(state))
        .map(|state| format!("{} inside {state}", served.display()))
}

/// Queue `path`'s components so that popping yields them in order; `.` is dropped.
fn push_components(pending: &mut Vec<OsString>, path: &Path) {
    let parts: Vec<OsString> = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(name) => Some(name.to_os_string()),
            Component::ParentDir => Some(OsString::from("..")),
            Component::CurDir | Component::RootDir | Component::Prefix(_) => None,
        })
        .collect();
    pending.extend(parts.into_iter().rev());
}

/// A directory's entries sorted by name, so which link first reaches a shared tree, and so the
/// paths recorded, depend on the tree alone.
fn listing(anchor: &OwnedFd, relative: &Path) -> Result<Vec<(OsString, EntryKind)>> {
    let mut entries = list_dir_no_follow(anchor, relative)?;
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(entries)
}

/// What `relative` is in itself, or `None` when it does not exist.
fn kind_at(anchor: &OwnedFd, relative: &Path) -> Result<Option<EntryKind>> {
    let stat = at_parent(anchor, relative, |dir, name| {
        match fstatat(dir, name, AtFlags::AT_SYMLINK_NOFOLLOW) {
            Ok(stat) => Ok(Some(stat)),
            Err(Errno::ENOENT) => Ok(None),
            Err(error) => Err(error),
        }
    })?;
    Ok(stat.map(|stat| kind_of_mode(stat.st_mode)))
}

/// Run `call` on `relative`'s parent, opened from the anchor without following any component,
/// and its name, so neither the entry nor a swapped parent is followed.
fn at_parent<T>(
    anchor: &OwnedFd,
    relative: &Path,
    call: impl FnOnce(BorrowedFd<'_>, &OsStr) -> nix::Result<T>,
) -> Result<T> {
    let name = relative
        .file_name()
        .with_context(|| format!("{} has no file name", relative.display()))?;
    let parent = relative.parent().unwrap_or(Path::new(""));
    if parent.as_os_str().is_empty() {
        return Ok(call(anchor.as_fd(), name)?);
    }
    let flags = libc::O_DIRECTORY | libc::O_RDONLY | libc::O_NONBLOCK;
    let dir = open_safely(anchor.as_raw_fd(), parent, flags, 0)?;
    Ok(call(dir.as_fd(), name)?)
}

#[cfg(test)]
mod tests;
