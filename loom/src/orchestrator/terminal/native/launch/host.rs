//! The host facts a launch resolves from the daemon's own environment before
//! anything is written: where claude is and which capsule flags it takes, the
//! project root, the verified loom hooks directory, the session scratch root,
//! and the daemon's own verified binary and filtered PATH, which the wrapper
//! exports as `LOOM_BIN` and `LOOM_HOOK_PATH`.
//!
//! [`LaunchHost::from_env`] is the only reader of the process environment;
//! everything downstream takes these facts as fields, which tests set
//! directly.

use anyhow::{bail, Context, Result};
use std::ffi::OsStr;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use super::super::capsule::{probed_capsule_support, CapsuleSupport};
use super::super::wrapper::{absolute, WrapperHostEnv};
use crate::relay::{ensure_dir_0700, session_dir};
use crate::sandbox::control_surfaces::{
    session_writable_roots, ControlSurfaces, WritableRootInputs,
};

/// Everything a launch needs from the host, resolved once.
pub(super) struct LaunchHost {
    pub(super) claude_path: PathBuf,
    pub(super) capsule_support: CapsuleSupport,
    /// The repository the work dir belongs to.
    pub(super) repo_root: PathBuf,
    /// The loom hooks directory, canonical and operator-owned; `None` when
    /// none is installed or it failed verification.
    pub(super) hooks_dir: Option<PathBuf>,
    pub(super) scratch_root: PathBuf,
    /// The operator's uid, which every verified path must be owned by.
    pub(super) uid: u32,
    /// The daemon's own binary, exported as `LOOM_BIN`.
    pub(super) loom_bin: PathBuf,
    /// The daemon's PATH minus every session-writable root, exported as
    /// `LOOM_HOOK_PATH`.
    pub(super) hook_path: Vec<PathBuf>,
    pub(super) home: Option<PathBuf>,
}

impl LaunchHost {
    /// Resolve every host fact from the daemon's process. A missing claude,
    /// an unresolvable scratch root, or a loom binary that is not an
    /// operator-owned regular file fails the spawn.
    pub(super) fn from_env(work_dir: &Path, cwd: &Path) -> Result<Self> {
        let claude_path = crate::claude::find_claude_path()?;
        // SAFETY: `getuid` has no preconditions and cannot fail.
        let uid = unsafe { libc::getuid() };
        let home = dirs::home_dir();
        let repo_root = project_root(work_dir).unwrap_or_else(|| absolute(cwd));
        let scratch_root = host_scratch_root(work_dir)?;
        let exe = std::env::current_exe().context("cannot locate the running loom binary")?;
        let roots = writable_roots(work_dir, &repo_root, &scratch_root, home.as_deref());
        let path_var = std::env::var_os("PATH").unwrap_or_default();
        Ok(Self {
            capsule_support: probed_capsule_support(&claude_path),
            claude_path,
            hooks_dir: verified_hooks_dir(uid),
            loom_bin: verified_loom_bin(&exe, uid)?,
            hook_path: hook_path_entries(&path_var, &roots),
            repo_root,
            scratch_root,
            uid,
            home,
        })
    }

    /// Create the scratch root and this session's `<root>/<session-id>`, both
    /// 0700 and owned by the operator. An unusable root fails the spawn.
    pub(super) fn prepare_scratch(&self, session_id: &str) -> Result<PathBuf> {
        ensure_dir_0700(&self.scratch_root, self.uid).with_context(|| {
            format!(
                "the session scratch root {} is unusable; refusing to spawn",
                self.scratch_root.display()
            )
        })?;
        let dir = session_dir(&self.scratch_root, session_id)?;
        ensure_dir_0700(&dir, self.uid).with_context(|| {
            format!(
                "cannot prepare the session scratch directory {}",
                dir.display()
            )
        })?;
        Ok(dir)
    }

    /// What the approved-permissions list rendered into this launch's capsule
    /// may never grant.
    pub(super) fn control_surfaces(&self, work_dir: &Path) -> ControlSurfaces {
        let hooks_dirs: Vec<PathBuf> = self.hooks_dir.iter().cloned().collect();
        ControlSurfaces::new(
            &absolute(work_dir),
            Some(&self.scratch_root),
            &hooks_dirs,
            self.home.as_deref(),
        )
    }

    /// The wrapper's `LOOM_SCRATCH_DIR`, `LOOM_BIN` and `LOOM_HOOK_PATH`.
    pub(super) fn wrapper_env(&self, scratch_dir: PathBuf) -> WrapperHostEnv {
        WrapperHostEnv {
            scratch_dir: Some(scratch_dir),
            loom_bin: Some(self.loom_bin.clone()),
            hook_path: self.hook_path.clone(),
        }
    }
}

/// The project root of `work_dir`, layout-aware (`WorkDir::project_root`).
fn project_root(work_dir: &Path) -> Option<PathBuf> {
    let work_dir = crate::fs::work_dir::WorkDir::new(absolute(work_dir)).ok()?;
    work_dir.project_root().map(Path::to_path_buf)
}

/// Where session scratch directories live.
///
/// Test builds keep them inside the test's own work dir, so no unit test that
/// drives a real launch creates directories in the operator's runtime or
/// cache directory; every other build takes the relay contract's root.
fn host_scratch_root(work_dir: &Path) -> Result<PathBuf> {
    if cfg!(test) {
        return Ok(absolute(work_dir).join("scratch"));
    }
    crate::relay::scratch_root_from_env().context("cannot resolve the session scratch root")
}

/// `find_hooks_dir()`'s answer when it canonicalizes to a directory the
/// operator owns; `None`, with a warning, otherwise.
fn verified_hooks_dir(uid: u32) -> Option<PathBuf> {
    let dir = crate::hooks::find_hooks_dir()?;
    match owned_entry(&dir, uid, true) {
        Ok(verified) => Some(verified),
        Err(error) => {
            tracing::warn!(dir = %dir.display(), %error, "ignoring an unverifiable loom hooks directory");
            None
        }
    }
}

/// The daemon's own binary, canonicalized and verified to be a regular file
/// owned by the operator.
pub(super) fn verified_loom_bin(exe: &Path, uid: u32) -> Result<PathBuf> {
    owned_entry(exe, uid, false).context("the running loom binary cannot be exported as LOOM_BIN")
}

/// `path` canonicalized and required to be a directory (`dir`) or a regular
/// file, owned by `uid`.
fn owned_entry(path: &Path, uid: u32, dir: bool) -> Result<PathBuf> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("cannot resolve {}", path.display()))?;
    let metadata = std::fs::metadata(&canonical)
        .with_context(|| format!("cannot stat {}", canonical.display()))?;
    if dir && !metadata.is_dir() {
        bail!("{} is not a directory", canonical.display());
    }
    if !dir && !metadata.is_file() {
        bail!("{} is not a regular file", canonical.display());
    }
    if metadata.uid() != uid {
        bail!(
            "{} is owned by uid {}, not the operator (uid {uid})",
            canonical.display(),
            metadata.uid()
        );
    }
    Ok(canonical)
}

/// Every root a session could write, over every stage in the plan (their
/// merged `allowWrite` entries, and whether any licenses the codex lane).
fn writable_roots(
    work_dir: &Path,
    repo_root: &Path,
    scratch_root: &Path,
    home: Option<&Path>,
) -> Vec<PathBuf> {
    let plan = crate::fs::work_dir::read_plan_sandbox(work_dir)
        .ok()
        .flatten()
        .unwrap_or_default();
    let mut allow_write = Vec::new();
    let mut codex_licensed = false;
    for stage in crate::verify::list_all_stages(work_dir).unwrap_or_default() {
        let mut merged = crate::sandbox::merge_config(
            &plan,
            &stage.sandbox,
            stage.stage_type,
            &stage.implementers,
        );
        crate::sandbox::expand_paths(&mut merged);
        codex_licensed |= merged.implementers.includes_codex();
        allow_write.extend(merged.filesystem.allow_write);
    }
    let tmpdir = std::env::var_os("TMPDIR").map(PathBuf::from);
    session_writable_roots(&WritableRootInputs {
        repo_root,
        allow_write: &allow_write,
        codex_licensed,
        scratch_root,
        home,
        tmpdir: tmpdir.as_deref(),
    })
}

/// `path_var`'s entries that are absolute, exist as directories, and
/// canonicalize outside every one of `writable_roots`, each kept in canonical
/// form, first occurrence winning. An entry that cannot be written back into a
/// PATH (non-UTF-8, or holding `:`) is dropped too.
pub(super) fn hook_path_entries(path_var: &OsStr, writable_roots: &[PathBuf]) -> Vec<PathBuf> {
    let roots: Vec<PathBuf> = writable_roots.iter().map(|root| absolute(root)).collect();
    let mut kept: Vec<PathBuf> = Vec::new();
    for entry in std::env::split_paths(path_var).filter(|entry| entry.is_absolute()) {
        let Ok(canonical) = entry.canonicalize() else {
            continue;
        };
        let representable = canonical.to_str().is_some_and(|text| !text.contains(':'));
        let writable = roots.iter().any(|root| canonical.starts_with(root));
        if canonical.is_dir() && representable && !writable && !kept.contains(&canonical) {
            kept.push(canonical);
        }
    }
    kept
}
