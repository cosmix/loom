//! Detect configuration edits made while Codex reviews a plan.

use std::collections::{BTreeMap, BTreeSet};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::fs::safe_read::EntryKind;

mod lookup;

use lookup::{Listing, Lookups};

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
/// entries, more than `lookup::MAX_WORK` path lookups, or a path longer than `MAX_PATH_BYTES`.
///
/// `.claude/worktrees/` is skipped when it is a real directory (see `CLAUDE_WORKTREES`). When it
/// is a symlink, or another link serves a directory under it, the link is configuration a
/// session could read and is handled like any other link.
pub fn snapshot(root: &Path) -> Result<ConfigSnapshot> {
    let mut walker = Walker::new(root)?;
    for (name, kind) in walker.lookups.listing(Path::new(""))? {
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

struct Walker {
    lookups: Lookups,
    /// Real directories whose trees are fingerprinted or queued; a link into one adds no walk.
    /// A set, so whether one covers a served directory costs a lookup per ancestor of it.
    walked: BTreeSet<PathBuf>,
    /// Real directories left out of a walked tree (`CLAUDE_WORKTREES`); a link into one still
    /// needs its own walk.
    skipped: Vec<PathBuf>,
    /// Directories still to list, as (path shown, real directory): one explicit stack, so a
    /// long link chain costs heap, never call depth.
    pending: Vec<(PathBuf, PathBuf)>,
    /// Entries visited so far, against `MAX_ENTRIES`.
    visited: usize,
    files: BTreeMap<String, String>,
}

impl Walker {
    fn new(root: &Path) -> Result<Self> {
        Ok(Walker {
            lookups: Lookups::new(root)?,
            walked: BTreeSet::new(),
            skipped: Vec::new(),
            pending: Vec::new(),
            visited: 0,
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
            self.walked.insert(served.clone());
            self.pending.push((shown.to_path_buf(), served));
        }
        Ok(())
    }

    /// Whether a walked or queued tree already fingerprints `served`: one that holds it and
    /// does not leave it out as a skipped directory (a tree walked from inside the skipped
    /// directory holds what is under it).
    fn covered(&self, served: &Path) -> bool {
        served.ancestors().any(|done| {
            self.walked.contains(done)
                && !self
                    .skipped
                    .iter()
                    .any(|left_out| served.starts_with(left_out) && !done.starts_with(left_out))
        })
    }

    /// Fingerprint every queued tree, including those the links it meets queue.
    fn walk(&mut self) -> Result<()> {
        while let Some((shown, real)) = self.pending.pop() {
            let mut entries = self
                .lookups
                .listing(&real)
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
    fn leave_out_worktrees(&mut self, shown: &Path, real: &Path, entries: &mut Listing) {
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
            EntryKind::File => self.lookups.digest(real),
            EntryKind::Symlink => self.link(shown, real),
            EntryKind::Special => Ok("special".to_owned()),
            EntryKind::Directory => Ok("directory".to_owned()),
        }
        .with_context(|| format!("Cannot snapshot {path}"))?;
        self.files.insert(path, fingerprint);
        Ok(())
    }

    /// A symlink's own target, then what it serves when it resolves inside the repository; a
    /// directory served at or inside `.claude/` is queued for walking.
    fn link(&mut self, shown: &Path, real: &Path) -> Result<String> {
        let target = self.lookups.target(real)?;
        let mut fingerprint = format!("symlink:{}", hex::encode(target.as_bytes()));
        match self.lookups.resolve(real)? {
            None => {}
            Some((served, EntryKind::File)) => {
                fingerprint.push_str(&format!(":file:{}", self.lookups.digest(&served)?));
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

#[cfg(test)]
mod tests;
