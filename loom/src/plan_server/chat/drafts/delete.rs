//! The files a confirmed kickoff's grant created, and their deletion.
//!
//! Every path derives from the draft's stored target, and only once that target is exactly
//! what [`kickoff_target`] makes of its own size and name. The draft file lives in the runtime
//! directory, where anyone with access can edit it, so the check is byte for byte and never
//! widened. Every filesystem access is anchored at a no-follow descriptor of the project root
//! through `safe_fs`: a symlinked directory on the way is refused, a symlink at a removed name is
//! unlinked as a link, and neither is followed.

use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use nix::errno::Errno;
use nix::fcntl::AtFlags;
use nix::sys::stat::fstatat;

use super::store::locked_spellings;
use crate::fs::safe_fs::{
    open_safely, safe_open_dirfd, safe_remove_empty_dir_in_workdir, safe_remove_in_workdir,
    safe_remove_tree_in_workdir,
};
use crate::fs::safe_read::is_not_found;
use crate::git::runner::run_git;
use crate::plan_session::scope::{kickoff_target, KickoffGrant, KickoffSize};
use crate::planning::kinds::is_feature_name;

const PLAN_PREFIX: &str = "doc/plans/PLAN-";
const PLAN_SUFFIX: &str = ".md";
const FEATURE_PREFIX: &str = "doc/product/";
const FEATURE_SUFFIX: &str = "/index.md";

/// The files a confirmed kickoff's grant created that exist now, document first, and whether
/// HEAD holds any of them.
pub(in crate::plan_server::chat) struct Written {
    /// The grant the target names, as a kickoff session holds it.
    pub grant: KickoffGrant,
    pub paths: Vec<String>,
    pub committed: bool,
}

/// A stored target that passed [`grant_of`]: the grant's document and, for a plan, its bundle.
struct Grant {
    size: KickoffSize,
    name: String,
    document: PathBuf,
    bundle: Option<PathBuf>,
}

/// Derive the grant's files from `target` (the draft's stored confirmed document) and report
/// those that exist. Err when `target` is not exactly a kickoff target.
pub(in crate::plan_server::chat) fn written(root: &Path, target: &str) -> Result<Written, String> {
    let grant = grant_of(target)?;
    let dirfd = open_root(root)?;
    let paths = existing(&dirfd, &grant)?;
    let committed = paths.iter().any(|path| held_by_head(root, path));
    let granted = KickoffGrant {
        size: grant.size,
        name: grant.name,
    };
    Ok(Written {
        grant: granted,
        paths,
        committed,
    })
}

/// Delete exactly the grant's files of `target`; returns the removed paths.
pub(in crate::plan_server::chat) fn remove_written(
    root: &Path,
    target: &str,
) -> Result<Vec<String>, String> {
    let grant = grant_of(target)?;
    let dirfd = open_root(root)?;
    refuse_locked(&dirfd, &grant)?;
    // Every path is looked up before anything goes, so a symlinked directory refuses the whole
    // delete instead of stopping it halfway.
    let found = existing(&dirfd, &grant)?;
    let feature_dir_present =
        grant.size == KickoffSize::Feature && present(&dirfd, &feature_dir(&grant))?;
    let fd = dirfd.as_raw_fd();
    let mut removed = Vec::new();
    if found.contains(&display(&grant.document)) {
        safe_remove_in_workdir(fd, &grant.document).map_err(|e| refusal(&grant.document, e))?;
        removed.push(display(&grant.document));
    }
    if let Some(bundle) = grant
        .bundle
        .as_ref()
        .filter(|b| found.contains(&display(b)))
    {
        remove_bundle(&dirfd, &grant, bundle, !removed.is_empty())?;
        removed.push(display(bundle));
    }
    if feature_dir_present {
        let dir = feature_dir(&grant);
        let gone = safe_remove_empty_dir_in_workdir(fd, &dir).map_err(|e| refusal(&dir, e))?;
        if gone {
            removed.push(display(&dir));
        }
    }
    Ok(removed)
}

/// Remove the plan's bundle. A `loom run` that renamed the document between the lookup and its
/// unlink makes that unlink a no-op (ENOENT reads as removed), so the locked spellings are looked
/// up again first: the bundle of an executing plan stays.
fn remove_bundle(
    dirfd: &OwnedFd,
    grant: &Grant,
    bundle: &Path,
    document_gone: bool,
) -> Result<(), String> {
    // A locked spelling means the document moved, not that it was deleted: the plain refusal.
    refuse_locked(dirfd, grant)?;
    safe_remove_tree_in_workdir(dirfd.as_raw_fd(), bundle).map_err(|error| {
        let refused = refusal(bundle, error);
        match document_gone {
            true => format!(
                "{refused}; {} is already deleted.",
                grant.document.display()
            ),
            false => refused,
        }
    })
}

/// Refuse when a locked spelling of the grant's document exists: the plan went into execution.
fn refuse_locked(dirfd: &OwnedFd, grant: &Grant) -> Result<(), String> {
    match locked_spelling(dirfd, grant)? {
        Some(locked) => Err(format!(
            "{} has gone into execution; it cannot be deleted here.",
            locked.display()
        )),
        None => Ok(()),
    }
}

/// The size `target` names, once it passed the same check as [`written`]. Err when `target` is
/// not exactly a kickoff target.
pub(in crate::plan_server::chat) fn target_size(target: &str) -> Result<KickoffSize, String> {
    grant_of(target).map(|grant| grant.size)
}

/// The grant `target` names, when `target` is byte for byte `kickoff_target(size, name)`'s
/// document for a size and a name `is_feature_name` accepts.
fn grant_of(target: &str) -> Result<Grant, String> {
    let parsed = if let Some(rest) = target.strip_prefix(PLAN_PREFIX) {
        rest.strip_suffix(PLAN_SUFFIX)
            .map(|name| (KickoffSize::Plan, name))
    } else if let Some(rest) = target.strip_prefix(FEATURE_PREFIX) {
        rest.strip_suffix(FEATURE_SUFFIX)
            .map(|name| (KickoffSize::Feature, name))
    } else {
        None
    };
    let refused = || format!("{target} is not a kickoff target; nothing was deleted.");
    let (size, name) = parsed
        .filter(|(_, name)| is_feature_name(name))
        .ok_or_else(refused)?;
    let armed = kickoff_target(size, name);
    if armed.document.as_os_str().as_bytes() != target.as_bytes() {
        return Err(refused());
    }
    Ok(Grant {
        size,
        name: name.to_owned(),
        document: armed.document,
        bundle: armed.bundle,
    })
}

fn open_root(root: &Path) -> Result<OwnedFd, String> {
    safe_open_dirfd(root).map_err(|error| format!("Cannot open the project: {error:#}"))
}

fn feature_dir(grant: &Grant) -> PathBuf {
    Path::new(FEATURE_PREFIX).join(&grant.name)
}

/// The grant's paths that exist, document first, then the bundle directory.
fn existing(dirfd: &OwnedFd, grant: &Grant) -> Result<Vec<String>, String> {
    let candidates = std::iter::once(&grant.document).chain(grant.bundle.as_ref());
    let mut found = Vec::new();
    for path in candidates {
        if present(dirfd, path)? {
            found.push(display(path));
        }
    }
    Ok(found)
}

/// The first locked spelling of the grant's document that exists (see [`locked_spellings`]).
fn locked_spelling(dirfd: &OwnedFd, grant: &Grant) -> Result<Option<PathBuf>, String> {
    for spelling in locked_spellings(&grant.document) {
        if present(dirfd, &spelling)? {
            return Ok(Some(spelling));
        }
    }
    Ok(None)
}

/// Whether `relpath` names an entry under `dirfd`, a symlink counting as itself. Its parent is
/// opened refusing a symlink at every component; an absent parent reads as absent.
fn present(dirfd: &OwnedFd, relpath: &Path) -> Result<bool, String> {
    let (Some(parent), Some(name)) = (relpath.parent(), relpath.file_name()) else {
        return Err(format!("{} names no entry", relpath.display()));
    };
    let flags = libc::O_DIRECTORY | libc::O_RDONLY | libc::O_NONBLOCK;
    let parent_fd = match open_safely(dirfd.as_raw_fd(), parent, flags, 0) {
        Ok(fd) => fd,
        Err(error) if is_not_found(&error) => return Ok(false),
        Err(error) => return Err(refusal(relpath, error)),
    };
    match fstatat(&parent_fd, name, AtFlags::AT_SYMLINK_NOFOLLOW) {
        Ok(_) => Ok(true),
        Err(Errno::ENOENT) => Ok(false),
        Err(error) => Err(refusal(relpath, error.into())),
    }
}

/// Whether HEAD holds `path`. No HEAD, or a git that fails, reads as not held.
fn held_by_head(root: &Path, path: &str) -> bool {
    let held = format!("HEAD:./{path}");
    run_git(&["cat-file", "-e", &held], root).is_ok_and(|output| output.status.success())
}

fn refusal(path: &Path, error: anyhow::Error) -> String {
    format!("Cannot delete {}: {error:#}", path.display())
}

/// A path built from validated ASCII components, as the repository-relative string it is.
fn display(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
