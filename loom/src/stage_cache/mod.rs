//! One persistent build-cache root per stage, outside its worktree, shared by
//! the stage's session, its workers and its confined host jobs. Each
//! ecosystem's cache variables point below the root ([`StageCache::environment`]);
//! host caches are read-only seeds ([`seed_read_roots`]) and toolchains
//! installed under the operator's home stay readable ([`toolchain_read_roots`]).
//!
//! The root is writable by the stage's own code, so it can poison only its own
//! stage. [`StageCache::ensure`] refuses a root planted in advance (a symlink
//! or another user's directory), so a stage never adopts a cache it did not
//! create. A base run gets a [`StageCache`] of its own, rooted beside its base
//! checkout where the stage cannot write (`base_checkout`).

pub mod base_checkout;
pub mod cleanup;
mod developer;
mod environment;
mod install_roots;
mod roots;
pub mod session;
mod writable;

#[cfg(test)]
mod contract_tests;
#[cfg(test)]
mod host_env_tests;
#[cfg(test)]
mod install_roots_tests;
#[cfg(test)]
mod kernel_tests;
#[cfg(test)]
mod policy_contract_tests;
#[cfg(test)]
mod session_contract_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod toolchain_tests;
#[cfg(test)]
mod writable_tests;

pub use developer::DeveloperTools;
pub use environment::{go_proxy, goproxy_has_userinfo, host_env_from, host_env_from_process};
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
    /// The directories from `base` down to the one holding the root, made by
    /// [`StageCache::ensure`]: [`PARENTS`] for a stage cache, the stage's
    /// base-run tree for a base run's cache (`base_checkout`).
    parents: Vec<String>,
    root: PathBuf,
    /// The consumer's private directory for a `Session` or `Worker` sccache
    /// socket ([`StageCache::with_socket_dir`]).
    socket_dir: Option<PathBuf>,
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
        Ok(StageCache {
            base,
            parents: PARENTS.map(String::from).to_vec(),
            root,
            socket_dir: None,
        })
    }

    /// [`StageCache::resolve`] under this process's `HOME` and
    /// `XDG_CACHE_HOME`, the one resolver every consumer and cleanup uses, so
    /// the CLI and the daemon (whose environment keeps both) name one root.
    pub fn resolve_from_process(repo_root: &Path, stage_id: &str) -> Result<StageCache> {
        let home = std::env::var_os("HOME")
            .filter(|home| !home.is_empty())
            .context("HOME is not set: the stage build cache lives below it")?;
        let xdg = std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from);
        StageCache::resolve(repo_root, stage_id, Path::new(&home), xdg.as_deref())
    }

    /// This cache with `dir` as the private directory of a `Session` or
    /// `Worker` sccache socket: one no other consumer of the stage is granted.
    /// Without it those consumers get no `SCCACHE_SERVER_UDS`.
    pub fn with_socket_dir(mut self, dir: &Path) -> StageCache {
        self.socket_dir = Some(dir.to_path_buf());
        self
    }

    /// This cache re-rooted at `root`, the canonical path [`StageCache::ensure`]
    /// returned, so the variables a consumer gets name the path its boundary
    /// grants (the configured root may reach it through a symlink).
    pub fn at_root(mut self, root: PathBuf) -> StageCache {
        self.root = root;
        self
    }

    /// The stage's cache root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Create the root 0700 (and each missing component from `<base>/loom`
    /// down), opening every component without following a symlink. Refuses a
    /// symlinked or foreign-owned component; an existing root is closed to
    /// this user. Entries already in the root are the stage's own and stay.
    ///
    /// Returns the canonical root: the canonical base joined with the fixed
    /// components just opened without following links. A boundary grants this
    /// path, never one resolved again later, which a stage could by then have
    /// swapped for a symlink (the root itself is the stage's to write).
    pub fn ensure(&self) -> Result<PathBuf> {
        let parents: Vec<&str> = self.parents.iter().map(String::as_str).collect();
        self.ensure_below(&parents)
    }

    /// The root's name, shared by the directories kept for this stage
    /// beside the stage cache (`base_checkout`).
    fn root_name(&self) -> Result<&std::ffi::OsStr> {
        self.root
            .file_name()
            .context("stage cache root has no name")
    }

    /// `<base>/<parents..>/<root name>`, made and opened as [`Self::ensure`]
    /// describes, closed to this user; returns its canonical path.
    fn ensure_below(&self, parents: &[&str]) -> Result<PathBuf> {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&self.base)
            .with_context(|| format!("create cache base {}", self.base.display()))?;
        let base = self
            .base
            .canonicalize()
            .with_context(|| format!("resolve cache base {}", self.base.display()))?;
        let flags = OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC;
        let mut directory = Dir::open(&base, flags, Mode::empty())
            .with_context(|| format!("open cache base {}", base.display()))?;
        let mut path = base;
        for name in parents {
            path.push(name);
            directory = open_owned(directory.as_fd(), *name, &path)?;
        }
        let name = self.root_name()?;
        path.push(name);
        let root = open_owned(directory.as_fd(), name, &path)?;
        if fstat(&root)?.st_mode & 0o777 != 0o700 {
            fchmod(&root, Mode::S_IRWXU).with_context(|| format!("close {}", path.display()))?;
        }
        Ok(path)
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
