//! One persistent build-cache root per stage, outside its worktree, shared by
//! the stage's session, its workers and its confined host jobs. Each
//! ecosystem's cache variables point below the root ([`StageCache::environment`]);
//! host caches are read-only seeds ([`seed_read_roots`]) and toolchains
//! installed under the operator's home stay readable ([`toolchain_read_roots`]).
//!
//! The root is writable by the stage's own code, so it can poison only its own
//! stage. [`StageCache::ensure`] refuses a root planted in advance (a symlink
//! or another user's directory), so a stage never adopts a cache it did not
//! create.

mod developer;
mod environment;
mod roots;

#[cfg(test)]
mod contract_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod toolchain_tests;

pub use developer::DeveloperTools;
pub use roots::{seed_read_roots, toolchain_read_roots};

use anyhow::{Context, Result};
use nix::dir::Dir;
use nix::errno::Errno;
use nix::fcntl::OFlag;
use nix::sys::stat::{fchmod, fstat, mkdirat, Mode};
use std::os::fd::{AsFd, BorrowedFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

/// Who runs with the stage cache; each starts its own sccache server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Consumer {
    Session,
    Worker,
    HostJob,
}

/// Directories from the cache base down to the root, created by [`StageCache::ensure`].
const PARENTS: [&str; 2] = ["loom", "stage-cache"];

/// Hex characters of the root's name: fixed, so `<root>/<socket>` stays short.
const ROOT_NAME_LEN: usize = 24;

/// A stage's cache root, `<base>/loom/stage-cache/<24 hex>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageCache {
    base: PathBuf,
    root: PathBuf,
}

impl StageCache {
    /// Resolve the stage's root without touching the filesystem beyond
    /// canonicalising `repo_root` (hashed as given when it does not exist).
    /// `<base>` is an absolute `xdg_cache_home`, else the platform cache
    /// directory under `home`.
    pub fn resolve(
        repo_root: &Path,
        stage_id: &str,
        home: &Path,
        xdg_cache_home: Option<&Path>,
    ) -> Result<StageCache> {
        anyhow::ensure!(
            !stage_id.is_empty()
                && !stage_id.contains('/')
                && !stage_id.contains('\0')
                && !stage_id.contains(".."),
            "stage id {stage_id:?} cannot name a stage cache"
        );
        let base = match xdg_cache_home.filter(|path| path.is_absolute()) {
            Some(xdg) => xdg.to_path_buf(),
            None => home.join(platform_cache_dir()),
        };
        anyhow::ensure!(
            base.is_absolute(),
            "stage cache base {} is not absolute",
            base.display()
        );
        let repo = repo_root
            .canonicalize()
            .unwrap_or_else(|_| repo_root.to_path_buf());
        let mut key = repo.as_os_str().as_bytes().to_vec();
        key.push(0);
        key.extend_from_slice(stage_id.as_bytes());
        let name = &crate::utils::sha256_hex(&key)[..ROOT_NAME_LEN];
        let mut root = base.clone();
        root.extend(PARENTS);
        root.push(name);
        Ok(StageCache { base, root })
    }

    /// The stage's cache root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Create the root 0700 (and each missing component from `<base>/loom`
    /// down), opening every component without following a symlink. Refuses a
    /// symlinked or foreign-owned component; an existing root is closed to
    /// this user. Entries already in the root are the stage's own and stay.
    pub fn ensure(&self) -> Result<()> {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&self.base)
            .with_context(|| format!("create cache base {}", self.base.display()))?;
        let flags = OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC;
        let mut directory = Dir::open(&self.base, flags, Mode::empty())
            .with_context(|| format!("open cache base {}", self.base.display()))?;
        let mut path = self.base.clone();
        for name in PARENTS {
            path.push(name);
            directory = open_owned(directory.as_fd(), name, &path)?;
        }
        let name = self
            .root
            .file_name()
            .context("stage cache root has no name")?;
        let root = open_owned(directory.as_fd(), name, &self.root)?;
        if fstat(&root)?.st_mode & 0o777 != 0o700 {
            fchmod(&root, Mode::S_IRWXU)
                .with_context(|| format!("close stage cache {}", self.root.display()))?;
        }
        Ok(())
    }
}

/// The per-user cache directory below HOME: `Library/Caches` on macOS,
/// `.cache` (the XDG default) elsewhere.
fn platform_cache_dir() -> &'static str {
    if cfg!(target_os = "macos") {
        "Library/Caches"
    } else {
        ".cache"
    }
}

/// Create `name` under `parent` (0700) when missing, then open it without
/// following a final symlink and require this user to own it.
fn open_owned<P>(parent: BorrowedFd<'_>, name: &P, path: &Path) -> Result<Dir>
where
    P: ?Sized + nix::NixPath,
{
    match mkdirat(parent, name, Mode::S_IRWXU) {
        Ok(()) | Err(Errno::EEXIST) => {}
        Err(error) => return Err(error).with_context(|| format!("create {}", path.display())),
    }
    let flags = OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC;
    let directory = Dir::openat(parent, name, flags, Mode::empty()).with_context(|| {
        format!(
            "stage cache path {} must be a directory, not a symlink",
            path.display()
        )
    })?;
    // SAFETY: `geteuid` has no preconditions and cannot fail.
    let user = unsafe { libc::geteuid() };
    anyhow::ensure!(
        fstat(&directory)?.st_uid == user,
        "stage cache path {} is not owned by this user",
        path.display()
    );
    Ok(directory)
}
