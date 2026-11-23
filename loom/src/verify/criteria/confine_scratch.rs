//! Private tool state for one confined command, kept outside its working tree:
//! recursive linters follow the Cargo source symlinks, and Biome's Git ignore
//! matcher panics on their external paths. Preparing a command creates its
//! empty scratch directory; the Cargo home inside it, or the per-tree Cargo
//! home, is filled at spawn. No repository inputs or host credentials are copied.
//!
//! Cargo fingerprints registry packages by their absolute source path, so the
//! Cargo home is the host's own path wherever the launcher allows it: another
//! path rebuilds every dependency ("the path to the source changed"). Temporary
//! directories stay per command.
//!
//! A command of a stage runs with the stage's persistent build caches
//! (`crate::stage_cache`), whose variables replace the cache directories here.
//! Without one, the package manager caches below stay per command: uv and Bun
//! read `UV_CACHE_DIR` and `BUN_INSTALL_CACHE_DIR` from this scratch. Under
//! bubblewrap each is an overlay whose lower is the host's default cache for
//! that tool, so an offline dependency present only there resolves while the
//! command's own writes stay in its private upper and go with its scratch.
//! Without overlay support (Seatbelt, bubblewrap before 0.11, or a filesystem
//! that refuses the mount) the cache starts empty, as `XDG_CACHE_HOME` always
//! does: such a command needs a `provision` entry instead.
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

use super::host::cargo_home;

#[cfg(unix)]
#[path = "confine_linked_home.rs"]
pub(super) mod linked_home;
#[cfg(target_os = "linux")]
#[path = "confine_scratch/overlay.rs"]
mod overlay;
#[path = "confine_scratch/reclaim.rs"]
mod reclaim;

pub(crate) struct CargoScratch {
    directory: tempfile::TempDir,
    cargo: CargoHome,
    #[cfg(target_os = "linux")]
    cache_lowers: Vec<(&'static str, PathBuf)>,
    /// Left on disk when dropped: see [`CargoScratch::keep`].
    kept: bool,
}

enum CargoHome {
    /// Bubblewrap binds `<scratch>/cargo` writable at the host's Cargo home,
    /// then the host's `bin`, `registry` and `git` read-only over it, so sources
    /// keep the paths the session's own `cargo` builds against and a target
    /// directory warmed outside the boundary stays warm inside it.
    Mounted(PathBuf),
    /// The host's Cargo home read in place, read-only, where the launcher cannot
    /// remap it (Seatbelt); Cargo only warns that its lock and last-use writes fail.
    Host(PathBuf),
    /// A directory linking the host's `registry` and `git`, for a Cargo home
    /// overlapping the tree: from [`CargoScratch::prepare`] one persistent
    /// directory per tree, outside it; from [`CargoScratch::prepare_private`]
    /// `<scratch>/cargo`, removed with the scratch.
    Linked {
        home: PathBuf,
        sources: Vec<(&'static str, PathBuf)>,
    },
}

impl CargoScratch {
    /// `mount` says the launcher can bind a private directory at the host's
    /// Cargo home path. `stage_cache` says a stage cache replaces the package
    /// manager caches, so no host cache is overlaid beneath them.
    pub(crate) fn prepare(
        command: &Command,
        tree: &Path,
        mount: bool,
        stage_cache: bool,
    ) -> Result<Self> {
        Self::build(command, tree, mount, false, !stage_cache)
    }

    /// A mounted scratch whose Cargo home, Linked or not, lives inside it and
    /// is removed with it. For a boundary whose commands may write their Cargo
    /// home: a home shared with later boundaries or the confined gate would let
    /// a planted `config.toml` (`build.rustc-wrapper`) run there unseen.
    #[cfg(any(target_os = "linux", test))]
    pub(crate) fn prepare_private(command: &Command, tree: &Path) -> Result<Self> {
        Self::build(command, tree, true, true, true)
    }

    fn build(
        command: &Command,
        tree: &Path,
        mount: bool,
        private: bool,
        #[cfg_attr(not(target_os = "linux"), allow(unused_variables))] overlays: bool,
    ) -> Result<Self> {
        let directory = private_temporary_directory(tree)?;
        let host = cargo_home(command);
        let cargo = match host {
            Some(host) if mount && !overlaps(&host, tree) => CargoHome::Mounted(host),
            Some(host) if !private && !overlaps(&host, tree) => CargoHome::Host(host),
            host => CargoHome::Linked {
                home: if private {
                    directory.path().join("cargo")
                } else {
                    stable_cargo_home(tree)?
                },
                sources: linked_sources(host.as_deref()),
            },
        };
        #[cfg(target_os = "linux")]
        let cache_lowers = if mount && overlays {
            overlay::supported_lowers(command, directory.path())
        } else {
            Vec::new()
        };
        Ok(Self {
            directory,
            cargo,
            #[cfg(target_os = "linux")]
            cache_lowers,
            kept: false,
        })
    }

    /// Record that this scratch is bound into a native-worker boundary of
    /// `pid_namespace` (`None` before bubblewrap reports it), so a sweep
    /// keeps it while that namespace may have members.
    #[cfg(target_os = "linux")]
    pub(crate) fn mark_boundary(&self, pid_namespace: Option<u64>) -> Result<()> {
        reclaim::marker::write(self.path(), pid_namespace)
    }

    /// Leave the scratch on disk when dropped: it is bound into a boundary
    /// whose namespace was not proven empty. The sweep reclaims it once its
    /// owner and that namespace are gone ([`CargoScratch::mark_boundary`]).
    #[cfg(target_os = "linux")]
    pub(crate) fn keep(&mut self) {
        self.kept = true;
        self.directory.disable_cleanup(true);
    }

    /// The per-command scratch root: temporary files and package-manager caches.
    pub(crate) fn path(&self) -> &Path {
        self.directory.path()
    }

    /// The directory for the host job's sccache socket, one only that job's
    /// own command can reach. Linux: `/tmp`, a private tmpfs per command under
    /// bubblewrap, so no session sees it and `/tmp/h.sock` always fits
    /// `sun_path`. macOS: the command's scratch, fresh and randomly named,
    /// which the Seatbelt profile lets the command bind and connect in.
    pub(super) fn sccache_socket_dir(&self) -> &Path {
        if cfg!(target_os = "linux") {
            Path::new("/tmp")
        } else {
            self.path()
        }
    }

    /// The private directory bound writable at the host Cargo home, and that path.
    #[cfg(target_os = "linux")]
    pub(super) fn cargo_mount(&self) -> Option<(PathBuf, &Path)> {
        match &self.cargo {
            CargoHome::Mounted(home) => Some((self.path().join("cargo"), home)),
            CargoHome::Linked { .. } | CargoHome::Host(_) => None,
        }
    }

    /// The Linked Cargo home the launcher must grant writes to: persistent per
    /// tree, or inside the scratch for [`CargoScratch::prepare_private`].
    pub(crate) fn linked_cargo_home(&self) -> Option<&Path> {
        match &self.cargo {
            CargoHome::Linked { home, .. } => Some(home),
            CargoHome::Mounted(_) | CargoHome::Host(_) => None,
        }
    }

    /// The host Cargo home the command reads in place ([`CargoHome::Host`]).
    #[cfg(target_os = "macos")]
    pub(super) fn host_cargo_home(&self) -> Option<&Path> {
        match &self.cargo {
            CargoHome::Host(home) => Some(home),
            CargoHome::Mounted(_) | CargoHome::Linked { .. } => None,
        }
    }

    pub(super) fn apply_environment(&self, command: &mut Command) {
        let cargo = match &self.cargo {
            CargoHome::Mounted(home) | CargoHome::Host(home) | CargoHome::Linked { home, .. } => {
                home
            }
        };
        command.env("CARGO_HOME", cargo);
        for key in ["TMPDIR", "TMP", "TEMP"] {
            command.env(key, self.path());
        }
        for (key, child) in [
            ("XDG_CACHE_HOME", "cache"),
            ("UV_CACHE_DIR", "uv-cache"),
            ("BUN_INSTALL_CACHE_DIR", "bun-cache"),
        ] {
            command.env(key, self.path().join(child));
        }
    }

    #[cfg(target_os = "linux")]
    pub(super) fn cache_overlays(&self) -> Vec<(PathBuf, PathBuf, PathBuf, PathBuf)> {
        self.cache_lowers
            .iter()
            .map(|(name, source)| {
                let root = self.path().join(format!("overlay-{name}"));
                (
                    source.clone(),
                    root.join("upper"),
                    root.join("work"),
                    self.path().join(name),
                )
            })
            .collect()
    }

    pub(crate) fn populate(&self) -> Result<()> {
        #[cfg(target_os = "linux")]
        for (_, upper, work, target) in self.cache_overlays() {
            for path in [upper, work, target] {
                std::fs::create_dir_all(path).context("create private cache overlay")?;
            }
        }
        match &self.cargo {
            CargoHome::Mounted(_) => std::fs::create_dir_all(self.path().join("cargo"))
                .context("create private Cargo home"),
            CargoHome::Host(_) => Ok(()),
            #[cfg(unix)]
            CargoHome::Linked { home, sources } => linked_home::populate(home, sources),
            #[cfg(not(unix))]
            CargoHome::Linked { .. } => Ok(()),
        }
    }
}

/// Restore owner access below the scratch (overlayfs `work/work` is 000, a
/// command may leave 0500 directories) before TempDir removes it, unless it
/// is kept. On Linux a scratch dropped while its thread panics is kept: the
/// unwinding skipped whatever would have proven its users stopped, and the
/// sweep reclaims it once its owner, and any boundary it marks, is gone.
#[cfg(unix)]
impl Drop for CargoScratch {
    fn drop(&mut self) {
        #[cfg(target_os = "linux")]
        if std::thread::panicking() {
            self.keep();
        }
        if !self.kept {
            reclaim::make_removable(self.path());
        }
    }
}

/// Whether one path contains the other: a Cargo home inside the tree, or a tree
/// inside the Cargo home, cannot be mounted over without hiding part of the tree.
fn overlaps(host: &Path, tree: &Path) -> bool {
    let host = host.canonicalize().unwrap_or_else(|_| host.to_path_buf());
    host.starts_with(tree) || tree.starts_with(&host)
}

/// The host's Cargo sources at the paths the launcher grants them: bubblewrap
/// binds `$CARGO_HOME/{registry,git}` as named, so a link to their resolved
/// path would dangle inside it when the Cargo home is itself a symlink.
/// Seatbelt grants the resolved roots, which these links reach.
fn linked_sources(host: Option<&Path>) -> Vec<(&'static str, PathBuf)> {
    let Some(cargo) = host else {
        return Vec::new();
    };
    ["registry", "git"]
        .into_iter()
        .map(|name| (name, cargo.join(name)))
        .filter(|(_, source)| source.is_dir())
        .collect()
}

/// Every root a command scratch may be created under, canonical.
fn candidate_roots() -> impl Iterator<Item = PathBuf> {
    [std::env::temp_dir(), "/tmp".into(), "/var/tmp".into()]
        .into_iter()
        .filter_map(|root| root.canonicalize().ok())
}

/// Roots for command scratch outside `tree`, in preference order. The default
/// comes first for inherited macOS temporary-directory grants; /var/tmp also
/// supports callers whose working tree is /tmp itself.
fn scratch_roots(tree: &Path) -> impl Iterator<Item = PathBuf> + '_ {
    outside(tree, candidate_roots())
}

/// The `roots` outside `tree`: a worker may write anywhere inside it.
fn outside<'a>(
    tree: &'a Path,
    roots: impl Iterator<Item = PathBuf> + 'a,
) -> impl Iterator<Item = PathBuf> + 'a {
    roots.filter(move |root| !root.starts_with(tree))
}

/// A fresh scratch outside `tree`, after starting the reclaim of orphaned
/// ones (once per process, in the background). On Linux it is created only
/// under a root that sweep enters, so an orphan never outlives its owner there.
fn private_temporary_directory(tree: &Path) -> Result<tempfile::TempDir> {
    #[cfg(target_os = "linux")]
    {
        reclaim::sweep_once(|| reclaim::sweepable(scratch_roots(tree)));
        reclaim::create(reclaim::sweepable(scratch_roots(tree)).into_iter())
    }
    #[cfg(not(target_os = "linux"))]
    reclaim::create(scratch_roots(tree))
}

/// `.loom-cargo-<digest of the canonical tree>` under the first scratch root.
fn stable_cargo_home(tree: &Path) -> Result<PathBuf> {
    let digest = crate::utils::sha256_hex(tree.as_os_str().as_encoded_bytes());
    let name = format!(".loom-cargo-{}", &digest[..24]);
    scratch_roots(tree)
        .next()
        .map(|root| root.join(name))
        .context("command Cargo home must be outside its working tree")
}

#[cfg(all(test, target_os = "linux"))]
#[path = "confine_scratch/cache_tests.rs"]
mod cache_tests;

#[cfg(all(test, unix))]
#[path = "confine_scratch/tests.rs"]
mod tests;
