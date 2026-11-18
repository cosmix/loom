//! The filesystem calls one snapshot makes, cached and counted against a work budget.
//!
//! Links that re-serve an ancestor make the walk meet the same real directories, links and
//! files again under longer shown paths. Each is looked up once per snapshot, so the cost
//! follows the real tree, and every lookup is charged what the portable component-by-component
//! open costs (one `openat` per path component; Linux `openat2` does it in one call), so the
//! budget bounds the work alike on every platform.

#[cfg(test)]
use std::cell::Cell;
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd};
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Context, Result};
use nix::errno::Errno;
use nix::fcntl::{readlinkat, AtFlags};
use nix::sys::stat::fstatat;

use crate::fs::safe_fs::{open_safely, safe_open_dirfd};
use crate::fs::safe_read::{kind_of_mode, list_dir_no_follow, read_bounded_at, EntryKind};

use super::{MAX_CONFIG_BYTES, MAX_ENTRIES, MAX_LINK_HOPS};

/// Work units one snapshot spends at most: one per path component a lookup opens, one per
/// component a link resolution steps through, one per cached answer. Ten per entry the entry
/// cap admits, far above any real `.claude/`; it stops trees whose links are cheap to list but
/// costly to resolve (long targets, deep link chains). Revisit with `MAX_ENTRIES`.
pub(super) const MAX_WORK: usize = 10 * MAX_ENTRIES;

/// A directory listing: names sorted, each with its kind.
pub(super) type Listing = Vec<(OsString, EntryKind)>;

/// A path's identity, its device and inode numbers: what names a directory whatever the
/// spelling of the path reaching it (a case-insensitive filesystem accepts many).
pub(super) type Identity = (libc::dev_t, libc::ino_t);

#[cfg(test)]
thread_local! {
    /// Work units the latest snapshot on this thread spent, so tests can bound its cost.
    pub(super) static SPENT: Cell<usize> = const { Cell::new(0) };
}

/// What a symlink finally serves inside the repository, and its kind; `None` when it does not.
pub(super) type Served = Option<(PathBuf, EntryKind)>;

pub(super) struct Lookups {
    anchor: OwnedFd,
    /// The repository root as given and canonicalized: an absolute link target under either is
    /// inside.
    spellings: Vec<PathBuf>,
    /// Units spent so far, against `MAX_WORK`.
    work: usize,
    listings: HashMap<PathBuf, Listing>,
    /// Kind and identity per real path, `None` when it does not exist.
    stats: HashMap<PathBuf, Option<(EntryKind, Identity)>>,
    targets: HashMap<PathBuf, OsString>,
    resolutions: HashMap<PathBuf, Served>,
    /// SHA-256 per real file, so many links serving one file read it once.
    digests: HashMap<PathBuf, String>,
}

impl Lookups {
    pub(super) fn new(root: &Path) -> Result<Self> {
        let anchor = safe_open_dirfd(root)?;
        let mut spellings = vec![root.to_path_buf()];
        let canonical = std::fs::canonicalize(root).ok();
        spellings.extend(canonical.filter(|canonical| canonical != root));
        Ok(Lookups {
            anchor,
            spellings,
            work: 0,
            listings: HashMap::new(),
            stats: HashMap::new(),
            targets: HashMap::new(),
            resolutions: HashMap::new(),
            digests: HashMap::new(),
        })
    }

    /// Spend `units`, failing closed past `MAX_WORK`.
    fn charge(&mut self, units: usize) -> Result<()> {
        self.work += units;
        #[cfg(test)]
        SPENT.with(|spent| spent.set(self.work));
        if self.work > MAX_WORK {
            bail!(
                "more than {MAX_WORK} configuration path lookups (symlinks re-serving \
                 directories?); session configuration cannot be checked"
            );
        }
        Ok(())
    }

    /// Charge one cached answer, or the components opening `relative` costs.
    fn charge_lookup(&mut self, relative: &Path, cached: bool) -> Result<()> {
        let units = if cached {
            1
        } else {
            relative.components().count().max(1)
        };
        self.charge(units)
    }

    /// The real directory `relative`'s entries sorted by name, so which link first reaches a
    /// shared tree, and so the paths recorded, depend on the tree alone.
    ///
    /// Every answer here is cached for the snapshot, so a path swapped while one snapshot runs
    /// may be seen in its first state only. That is acceptable: the snapshot compared against
    /// the one taken before Codex ran is taken only once Codex has exited
    /// (`CodexHalf::configuration_changed` in `pressure/codex.rs` returns early while it runs;
    /// the summary job's `guarded_outcome` runs after `Child::wait` returns), so the writer the
    /// guard watches for is no longer writing. Revisit if a snapshot ever runs beside Codex.
    pub(super) fn listing(&mut self, relative: &Path) -> Result<Listing> {
        self.charge_lookup(relative, self.listings.contains_key(relative))?;
        if let Some(known) = self.listings.get(relative) {
            return Ok(known.clone());
        }
        let mut entries = list_dir_no_follow(&self.anchor, relative)?;
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        self.listings
            .insert(relative.to_path_buf(), entries.clone());
        Ok(entries)
    }

    pub(super) fn digest(&mut self, real: &Path) -> Result<String> {
        self.charge_lookup(real, self.digests.contains_key(real))?;
        if let Some(known) = self.digests.get(real) {
            return Ok(known.clone());
        }
        let bytes = read_bounded_at(&self.anchor, real, MAX_CONFIG_BYTES)?;
        let digest = crate::utils::sha256_hex(&bytes);
        self.digests.insert(real.to_path_buf(), digest.clone());
        Ok(digest)
    }

    /// The target the symlink at the real path `link` holds.
    pub(super) fn target(&mut self, link: &Path) -> Result<OsString> {
        self.charge_lookup(link, self.targets.contains_key(link))?;
        if let Some(known) = self.targets.get(link) {
            return Ok(known.clone());
        }
        let target = at_parent(&self.anchor, link, |dir, name| readlinkat(dir, name))?;
        self.targets.insert(link.to_path_buf(), target.clone());
        Ok(target)
    }

    /// What the real path `relative` is in itself, or `None` when it does not exist.
    fn kind(&mut self, relative: &Path) -> Result<Option<EntryKind>> {
        Ok(self.stat(relative)?.map(|(kind, _)| kind))
    }

    /// The identity of the real path `relative` in itself, or `None` when it does not exist.
    pub(super) fn identity(&mut self, relative: &Path) -> Result<Option<Identity>> {
        Ok(self.stat(relative)?.map(|(_, identity)| identity))
    }

    fn stat(&mut self, relative: &Path) -> Result<Option<(EntryKind, Identity)>> {
        self.charge_lookup(relative, self.stats.contains_key(relative))?;
        if let Some(known) = self.stats.get(relative) {
            return Ok(*known);
        }
        let stat = stat_at(&self.anchor, relative)?;
        self.stats.insert(relative.to_path_buf(), stat);
        Ok(stat)
    }

    /// The repository-relative path the symlink `link` finally serves, and its kind. Resolved
    /// one component at a time from the anchor, each read without following it, so `..` after
    /// a real directory is its real parent. `None` for a dangling link, a component that is
    /// not a directory, a loop or over-long chain, or a target outside the repository.
    pub(super) fn resolve(&mut self, link: &Path) -> Result<Served> {
        self.charge_lookup(link, self.resolutions.contains_key(link))?;
        if let Some(known) = self.resolutions.get(link) {
            return Ok(known.clone());
        }
        let served = self.resolve_uncached(link)?;
        self.resolutions.insert(link.to_path_buf(), served.clone());
        Ok(served)
    }

    fn resolve_uncached(&mut self, link: &Path) -> Result<Served> {
        let mut resolved = PathBuf::new();
        let mut pending = Vec::new();
        push_components(&mut pending, link);
        let mut hops = 0;
        while let Some(part) = pending.pop() {
            self.charge(1)?;
            if part == ".." {
                if !resolved.pop() {
                    return Ok(None);
                }
                continue;
            }
            let candidate = resolved.join(&part);
            match self.kind(&candidate)? {
                None => return Ok(None),
                Some(EntryKind::Directory) => resolved = candidate,
                Some(EntryKind::Symlink) => {
                    hops += 1;
                    if hops > MAX_LINK_HOPS {
                        return Ok(None);
                    }
                    let target = PathBuf::from(self.target(&candidate)?);
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

/// What `relative` is in itself and its identity, or `None` when it does not exist.
fn stat_at(anchor: &OwnedFd, relative: &Path) -> Result<Option<(EntryKind, Identity)>> {
    let stat = at_parent(anchor, relative, |dir, name| {
        match fstatat(dir, name, AtFlags::AT_SYMLINK_NOFOLLOW) {
            Ok(stat) => Ok(Some(stat)),
            Err(Errno::ENOENT) => Ok(None),
            Err(error) => Err(error),
        }
    })?;
    Ok(stat.map(|stat| (kind_of_mode(stat.st_mode), (stat.st_dev, stat.st_ino))))
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
