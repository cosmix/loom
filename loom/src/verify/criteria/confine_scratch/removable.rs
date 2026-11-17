//! Restoring owner access below a scratch so a recursive removal can empty
//! each directory. The walk runs on directory descriptors and follows no
//! symlink, at the root or below it: a process still running in a boundary
//! the scratch is bound into may swap a directory for a link to `$HOME`
//! mid-walk, and that link is skipped, never entered or changed.
#[cfg(target_os = "linux")]
use std::ffi::CStr;
use std::ffi::{CString, OsStr};
use std::os::fd::{AsFd, BorrowedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use nix::dir::{Dir, Type};
use nix::errno::Errno;
use nix::fcntl::AT_FDCWD;
use nix::NixPath;

/// Deepest level restored below the root. The walk holds one descriptor per
/// level; a directory locked deeper stays locked and its removal fails with a
/// warning. Revisit if a tool's cache nests read-only directories deeper.
const MAX_DEPTH: usize = 128;

/// Give the owner `rwx` on every directory at and below `root`. A symlink is
/// never followed, `root` itself included; a missing `root` is no error.
pub(in super::super) fn make_removable(root: &Path) {
    match open_restored(AT_FDCWD, root) {
        Ok(dir) => restore_below(dir, root, 0),
        Err(error) => warn_unless_absent(root, error),
    }
}

/// Restore every subdirectory of `dir` (at `path`, for warnings) recursively.
fn restore_below(mut dir: Dir, path: &Path, depth: usize) {
    if depth >= MAX_DEPTH {
        tracing::warn!(path = %path.display(), "scratch nests too deep to restore permissions");
        return;
    }
    let names: Vec<CString> = dir
        .iter()
        .flatten()
        .filter(|entry| {
            let name = entry.file_name().to_bytes();
            let directory = matches!(entry.file_type(), Some(Type::Directory) | None);
            directory && name != b"." && name != b".."
        })
        .map(|entry| entry.file_name().to_owned())
        .collect();
    #[cfg(test)]
    listed::call(path);
    for name in names {
        let child = || path.join(OsStr::from_bytes(name.to_bytes()));
        match open_restored(dir.as_fd(), name.as_c_str()) {
            Ok(subdirectory) => restore_below(subdirectory, &child(), depth + 1),
            Err(error) => warn_unless_absent(&child(), error),
        }
    }
}

/// A removed entry, or one that is no directory (a symlink included), needs
/// nothing; anything else could not be restored.
fn warn_unless_absent(path: &Path, error: Errno) {
    if !matches!(error, Errno::ENOENT | Errno::ENOTDIR | Errno::ELOOP) {
        tracing::warn!(path = %path.display(), %error, "cannot restore scratch permissions");
    }
}

/// `name` under `parent`, opened without following a symlink, given owner
/// `rwx`, and opened for listing. Linux opens it `O_PATH` first, so a 000
/// directory can be restored; the mode is changed through the descriptor's
/// `/proc/self/fd` link, which names the opened directory, never a path.
#[cfg(target_os = "linux")]
fn open_restored<P: ?Sized + NixPath>(parent: BorrowedFd<'_>, name: &P) -> nix::Result<Dir> {
    use nix::fcntl::{openat, OFlag};
    use nix::sys::stat::{fchmodat, fstat, FchmodatFlags, Mode};
    use std::os::fd::AsRawFd;

    let flags = OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC;
    let handle = openat(parent, name, OFlag::O_PATH | flags, Mode::empty())?;
    let mode = fstat(&handle)?.st_mode;
    if mode & 0o700 != 0o700 {
        let link = format!("/proc/self/fd/{}", handle.as_raw_fd());
        let restored = Mode::from_bits_truncate((mode & 0o7777) | 0o700);
        fchmodat(
            AT_FDCWD,
            link.as_str(),
            restored,
            FchmodatFlags::FollowSymlink,
        )?;
    }
    let dot: &CStr = c".";
    Dir::from_fd(openat(
        &handle,
        dot,
        OFlag::O_RDONLY | flags,
        Mode::empty(),
    )?)
}

/// `name` under `parent`, opened without following a symlink, given owner
/// `rwx`. Without `O_PATH` a directory its owner cannot read stays locked.
#[cfg(not(target_os = "linux"))]
fn open_restored<P: ?Sized + NixPath>(parent: BorrowedFd<'_>, name: &P) -> nix::Result<Dir> {
    use nix::fcntl::{openat, OFlag};
    use nix::sys::stat::{fchmod, fstat, Mode};

    let flags = OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC;
    let directory = openat(parent, name, flags, Mode::empty())?;
    let mode = fstat(&directory)?.st_mode;
    if mode & 0o700 != 0o700 {
        fchmod(
            &directory,
            Mode::from_bits_truncate((mode & 0o7777) | 0o700),
        )?;
    }
    Dir::from_fd(directory)
}

/// A test's hook, run on its own thread after a directory is listed and
/// before its children are visited: where a live process could swap one.
#[cfg(test)]
pub(super) mod listed {
    use std::cell::RefCell;
    use std::path::Path;

    type Hook = Box<dyn FnMut(&Path)>;

    thread_local! {
        static HOOK: RefCell<Option<Hook>> = const { RefCell::new(None) };
    }

    /// Run `hook` with each listed directory's path during `body`.
    #[cfg(target_os = "linux")]
    pub(in super::super) fn with(hook: impl FnMut(&Path) + 'static, body: impl FnOnce()) {
        HOOK.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
        body();
        HOOK.with(|slot| slot.borrow_mut().take());
    }

    pub(super) fn call(path: &Path) {
        HOOK.with(|slot| {
            if let Some(hook) = slot.borrow_mut().as_mut() {
                hook(path);
            }
        });
    }
}
