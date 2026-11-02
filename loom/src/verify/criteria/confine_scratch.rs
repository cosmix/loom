//! Private tool state for one confined command, kept outside its working tree:
//! recursive linters follow the Cargo source symlinks, and Biome's Git ignore
//! matcher panics on their external paths. Preparing a command creates its
//! empty scratch directory; the Cargo home inside it, or the per-tree Cargo
//! home, is filled at spawn. No repository inputs or host credentials are copied.
//!
//! Cargo fingerprints registry packages by their absolute source path, so the
//! Cargo home must not change from one command to the next: a fresh one per
//! command marks every dependency dirty and rebuilds it. Temporary and package
//! manager cache directories stay per command.
//!
//! uv and Bun read their configured caches, `UV_CACHE_DIR` and
//! `BUN_INSTALL_CACHE_DIR`, from this scratch. Under bubblewrap each is an
//! overlay whose lower is the host's default cache for that tool, so an offline
//! dependency present only there resolves while the command's own writes stay in
//! its private upper and go with its scratch. Without overlay support (Seatbelt,
//! bubblewrap before 0.11, or a filesystem that refuses the mount) the cache
//! starts empty: such a command needs a `provision` entry instead.
//!
//! `XDG_CACHE_HOME` starts empty every time. A cache kept for the next command
//! would have to live outside the working tree where a stage session could write
//! it, and tools there can certify results (Go's test cache) that verification
//! must produce itself. Revisit if a stage-unwritable location becomes available.
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;

use super::host::cargo_home;

#[cfg(unix)]
#[path = "confine_linked_home.rs"]
mod linked_home;

pub(super) struct CargoScratch {
    directory: tempfile::TempDir,
    cargo: CargoHome,
    #[cfg(target_os = "linux")]
    cache_lowers: Vec<(&'static str, PathBuf)>,
}

enum CargoHome {
    /// Bubblewrap binds `<scratch>/cargo` writable at the host's Cargo home,
    /// then the host's `bin`, `registry` and `git` read-only over it, so sources
    /// keep the paths the session's own `cargo` builds against and a target
    /// directory warmed outside the boundary stays warm inside it.
    Mounted(PathBuf),
    /// A stable directory per working tree, outside it, linking the host's
    /// `registry` and `git`. Used where paths cannot be remapped (Seatbelt). It
    /// persists across commands and is never removed: one small directory per
    /// tree.
    Linked {
        home: PathBuf,
        sources: Vec<(&'static str, PathBuf)>,
    },
}

impl CargoScratch {
    /// `mount` says the launcher can bind a private directory at the host's
    /// Cargo home path.
    pub(super) fn prepare(command: &Command, tree: &Path, mount: bool) -> Result<Self> {
        let directory = private_temporary_directory(tree)?;
        let host = cargo_home(command);
        let cargo = match host {
            Some(host) if mount && !overlaps(&host, tree) => CargoHome::Mounted(host),
            host => CargoHome::Linked {
                home: stable_cargo_home(tree)?,
                sources: linked_sources(host.as_deref()),
            },
        };
        #[cfg(target_os = "linux")]
        let cache_lowers = if mount {
            cache_lowers(command)
                .into_iter()
                .filter(|(name, source)| {
                    let supported = overlay_supported(directory.path(), name, source);
                    if !supported {
                        tracing::warn!(
                            cache = name,
                            "cache overlay unavailable; using empty private cache"
                        );
                    }
                    supported
                })
                .collect()
        } else {
            Vec::new()
        };
        Ok(Self {
            directory,
            cargo,
            #[cfg(target_os = "linux")]
            cache_lowers,
        })
    }

    /// The per-command scratch root: temporary files and package-manager caches.
    pub(super) fn path(&self) -> &Path {
        self.directory.path()
    }

    /// The private directory bound writable at the host Cargo home, and that path.
    #[cfg(target_os = "linux")]
    pub(super) fn cargo_mount(&self) -> Option<(PathBuf, &Path)> {
        match &self.cargo {
            CargoHome::Mounted(home) => Some((self.path().join("cargo"), home)),
            CargoHome::Linked { .. } => None,
        }
    }

    /// The persistent per-tree Cargo home the launcher must grant writes to.
    pub(super) fn linked_cargo_home(&self) -> Option<&Path> {
        match &self.cargo {
            CargoHome::Linked { home, .. } => Some(home),
            CargoHome::Mounted(_) => None,
        }
    }

    pub(super) fn apply_environment(&self, command: &mut Command) {
        let cargo = match &self.cargo {
            CargoHome::Mounted(home) | CargoHome::Linked { home, .. } => home,
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

    pub(super) fn populate(&self) -> Result<()> {
        #[cfg(target_os = "linux")]
        for (_, upper, work, target) in self.cache_overlays() {
            for path in [upper, work, target] {
                std::fs::create_dir_all(path).context("create private cache overlay")?;
            }
        }
        match &self.cargo {
            CargoHome::Mounted(_) => std::fs::create_dir_all(self.path().join("cargo"))
                .context("create private Cargo home"),
            #[cfg(unix)]
            CargoHome::Linked { home, sources } => linked_home::populate(home, sources),
            #[cfg(not(unix))]
            CargoHome::Linked { .. } => Ok(()),
        }
    }
}

#[cfg(target_os = "linux")]
impl Drop for CargoScratch {
    fn drop(&mut self) {
        for (_, _, work, _) in self.cache_overlays() {
            restore_overlay_work_permissions(&work);
        }
    }
}

/// Overlayfs leaves `work/work` mode 000 after unmount. Restore owner access
/// before TempDir recursively removes the private scratch directory.
#[cfg(target_os = "linux")]
fn restore_overlay_work_permissions(work: &Path) {
    use std::os::unix::fs::PermissionsExt;

    let internal = work.join("work");
    let Ok(metadata) = std::fs::symlink_metadata(&internal) else {
        return;
    };
    if metadata.file_type().is_dir() {
        if let Err(error) =
            std::fs::set_permissions(&internal, std::fs::Permissions::from_mode(0o700))
        {
            tracing::warn!(path = %internal.display(), %error, "cannot restore overlay work permissions");
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

/// Roots for command scratch outside `tree`, in preference order. The default
/// comes first for inherited macOS temporary-directory grants; /var/tmp also
/// supports callers whose working tree is /tmp itself.
fn scratch_roots(tree: &Path) -> impl Iterator<Item = PathBuf> + '_ {
    [std::env::temp_dir(), "/tmp".into(), "/var/tmp".into()]
        .into_iter()
        .filter_map(|root| root.canonicalize().ok())
        .filter(move |root| !root.starts_with(tree))
}

fn private_temporary_directory(tree: &Path) -> Result<tempfile::TempDir> {
    let mut failure = None;
    for root in scratch_roots(tree) {
        match tempfile::Builder::new()
            .prefix(".loom-command-")
            .tempdir_in(&root)
        {
            Ok(directory) => return Ok(directory),
            Err(error) => failure = Some(error),
        }
    }
    match failure {
        Some(error) => {
            Err(error).context("create private command scratch outside its working tree")
        }
        None => anyhow::bail!("command scratch must be outside its working tree"),
    }
}

/// `.loom-cargo-<digest of the canonical tree>` under the first scratch root.
fn stable_cargo_home(tree: &Path) -> Result<PathBuf> {
    let digest = Sha256::digest(tree.as_os_str().as_encoded_bytes());
    let name = format!(".loom-cargo-{}", hex::encode(&digest[..12]));
    scratch_roots(tree)
        .next()
        .map(|root| root.join(name))
        .context("command Cargo home must be outside its working tree")
}

#[cfg(target_os = "linux")]
fn cache_lowers(command: &Command) -> Vec<(&'static str, PathBuf)> {
    let value = |key: &str| super::host::environment_path(command, key);
    // Reuse only roots already exposed by the host policy. An arbitrary
    // command-provided cache path must not grant access to private host files.
    let Some(home) = value("HOME") else {
        return Vec::new();
    };
    let uv = home.join(".cache/uv");
    let bun = home.join(".bun/install/cache");
    let uv_requested =
        value("UV_CACHE_DIR").or_else(|| value("XDG_CACHE_HOME").map(|p| p.join("uv")));
    let bun_requested = value("BUN_INSTALL_CACHE_DIR");
    [
        ("uv-cache", uv, uv_requested),
        ("bun-cache", bun, bun_requested),
    ]
    .into_iter()
    .filter_map(|(name, default, requested)| {
        let source = default.canonicalize().ok().filter(|p| p.is_dir())?;
        if requested.is_some_and(|p| p.canonicalize().ok().as_ref() != Some(&source)) {
            tracing::warn!(
                cache = name,
                "custom cache is outside the allowed host roots; using empty private cache"
            );
            return None;
        }
        Some((name, source))
    })
    .collect()
}

/// Test this lower and upper filesystem together; overlay support varies by mount.
/// The probe reads the cache but its private namespace never writes to it.
#[cfg(target_os = "linux")]
fn overlay_supported(scratch: &Path, name: &str, source: &Path) -> bool {
    let Ok(bwrap) = which::which("bwrap") else {
        return false;
    };
    let root = scratch.join(format!("overlay-{name}"));
    let upper = root.join("upper");
    let work = root.join("work");
    let target = scratch.join(name);
    if [&upper, &work, &target]
        .into_iter()
        .any(|p| std::fs::create_dir_all(p).is_err())
    {
        return false;
    }
    use wait_timeout::ChildExt;
    let child = Command::new(bwrap)
        .args([
            "--die-with-parent",
            "--unshare-all",
            "--ro-bind",
            "/",
            "/",
            "--overlay-src",
        ])
        .arg(source)
        .arg("--overlay")
        .arg(&upper)
        .arg(&work)
        .arg(&target)
        .args(["--", "/usr/bin/true"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    let Ok(mut child) = child else {
        return false;
    };
    let supported = match child.wait_timeout(std::time::Duration::from_secs(3)) {
        Ok(Some(status)) => status.success(),
        _ => {
            let _ = child.kill();
            let _ = child.wait();
            false
        }
    };
    restore_overlay_work_permissions(&work);
    supported
}

#[cfg(all(test, target_os = "linux"))]
#[path = "confine_scratch/cache_tests.rs"]
mod cache_tests;
