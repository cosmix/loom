//! Per-project planning runtime paths.
//!
//! Path computation, plus [`create_runtime_dir`] as the one place that creates the directory.
//! Outside a planning session the runtime directory is `<loom user dir>/plan/<project key>`;
//! inside one, `LOOM_PLAN_STATE` names it and must pass a set of provenance checks.

use std::ffi::OsString;
use std::fs::{File, Permissions};
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use nix::unistd::{access, AccessFlags};

use crate::context::source_graph::body_hash;
use crate::fs::safe_fs::{open_safely, safe_create_dir_all, safe_open_dirfd};

/// Environment variable a planning session uses to name its runtime directory.
const STATE_VAR: &str = "LOOM_PLAN_STATE";
/// Environment variable carrying the planning session id.
const SESSION_VAR: &str = "LOOM_PLAN_SESSION";
/// Name of the drafts directory inside the runtime directory.
pub(crate) const DRAFTS_DIR: &str = "drafts";
/// Name of the uploaded chat attachments directory inside the runtime directory.
pub(crate) const ATTACHMENTS_DIR: &str = "attachments";

/// Every runtime path of one project's planning state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimePaths {
    /// `<loom user dir>/plan`, the parent of every project's runtime directory.
    pub planning_root: PathBuf,
    /// The project's runtime directory.
    pub dir: PathBuf,
    /// The server lock file.
    pub lock: PathBuf,
    /// Directory of in-progress drafts.
    pub drafts: PathBuf,
    /// Directory of claims.
    pub claims: PathBuf,
    /// Directory of uploaded chat attachments (read-only to a planning session).
    pub attachments: PathBuf,
    /// Lock file serialising summary writes.
    pub summaries_lock: PathBuf,
    /// Append-only summary cost ledger.
    pub summary_cost: PathBuf,
    /// Directory of per-session state.
    pub sessions: PathBuf,
}

impl RuntimePaths {
    fn new(planning_root: PathBuf, dir: PathBuf) -> Self {
        RuntimePaths {
            lock: dir.join("lock"),
            drafts: dir.join(DRAFTS_DIR),
            claims: dir.join("claims"),
            attachments: dir.join(ATTACHMENTS_DIR),
            summaries_lock: dir.join("summaries.lock"),
            summary_cost: dir.join("summary-cost.jsonl"),
            sessions: dir.join("sessions"),
            planning_root,
            dir,
        }
    }
}

/// The stable key of a project: `<sanitised dir name>-<12 hex of sha256(canonical root)>`.
pub fn project_key(root: &Path) -> Result<String> {
    let canonical = root
        .canonicalize()
        .with_context(|| format!("canonicalising project root {}", root.display()))?;
    let name: String = canonical
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let hash = body_hash(canonical.as_os_str().as_encoded_bytes());
    let hex = hash.strip_prefix("sha256:").unwrap_or(&hash);
    Ok(format!("{name}-{}", &hex[..12]))
}

/// Runtime paths for `root` under `loom_dir`, ignoring the environment.
pub fn runtime_paths_under(loom_dir: &Path, root: &Path) -> Result<RuntimePaths> {
    runtime_paths_from(None, None, loom_dir, root)
}

/// Runtime paths from an optional session `state` directory and `session` id.
///
/// With `state` set it must pass every `LOOM_PLAN_STATE` provenance check and then
/// is the runtime directory; otherwise the directory is `<loom_dir>/plan/<key>`.
pub fn runtime_paths_from(
    state: Option<&Path>,
    session: Option<&str>,
    loom_dir: &Path,
    root: &Path,
) -> Result<RuntimePaths> {
    let key = project_key(root)?;
    let Some(state) = state else {
        let planning_root = loom_dir.join("plan");
        let dir = planning_root.join(&key);
        return Ok(RuntimePaths::new(planning_root, dir));
    };
    let parent = check_state(state, session, &key)?;
    Ok(RuntimePaths::new(parent, state.to_path_buf()))
}

/// Refuse `state` unless it is a legitimate session runtime directory; returns its parent.
fn check_state(state: &Path, session: Option<&str>, key: &str) -> Result<PathBuf> {
    if !state.is_absolute() {
        bail!("{STATE_VAR} must be an absolute path: {}", state.display());
    }
    let meta = std::fs::symlink_metadata(state)
        .with_context(|| format!("{STATE_VAR} {} cannot be inspected", state.display()))?;
    if !meta.file_type().is_dir() {
        bail!(
            "{STATE_VAR} must be a real directory, not a symlink or file: {}",
            state.display()
        );
    }
    // SAFETY: `getuid` has no preconditions and cannot fail.
    if meta.uid() != unsafe { libc::getuid() } {
        bail!(
            "{STATE_VAR} directory is not owned by the current user: {}",
            state.display()
        );
    }
    if state.file_name().and_then(|n| n.to_str()) != Some(key) {
        bail!("{STATE_VAR} must be named {key}: {}", state.display());
    }
    let session_ok = session.is_some_and(|s| {
        (8..=64).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
    });
    if !session_ok {
        bail!("{STATE_VAR} requires a well-formed {SESSION_VAR} (8 to 64 hex digits or dashes)");
    }
    let parent = state
        .parent()
        .ok_or_else(|| anyhow!("{STATE_VAR} has no parent directory: {}", state.display()))?;
    check_parent(state, parent, meta.dev())?;
    Ok(parent.to_path_buf())
}

/// Refuse a `parent` the caller could have made itself, which could bypass the server's lock
/// and claims.
///
/// Under Seatbelt the real planning root is unwritable, so a write test settles it. Under srt
/// on Linux the read-denied planning root is a private tmpfs per command and the session's
/// grants are bound into it from the host, so the parent is writable but `drafts`, `claims` and
/// `summaries.lock` each sit on another device than the state directory. A directory the caller
/// made holds no mounts.
fn check_parent(state: &Path, parent: &Path, state_dev: u64) -> Result<()> {
    if access(parent, AccessFlags::W_OK).is_err() || grants_mounted(state, parent, state_dev) {
        return Ok(());
    }
    bail!(
        "{STATE_VAR} parent {} is writable and its runtime grants are not mounted into it, so it \
         is not the planning root",
        parent.display()
    )
}

/// Whether `drafts/` and `claims/` (directories) and `summaries.lock` (a regular file) under
/// `state` are all on a device other than `state_dev`. `symlink_metadata` keeps a symlink to
/// another filesystem from counting; any error counts as not mounted.
fn grants_mounted(state: &Path, parent: &Path, state_dev: u64) -> bool {
    let paths = RuntimePaths::new(parent.to_path_buf(), state.to_path_buf());
    let elsewhere = |path: &Path, is_dir: bool| {
        std::fs::symlink_metadata(path).is_ok_and(|meta| {
            let kind = meta.file_type();
            (if is_dir {
                kind.is_dir()
            } else {
                kind.is_file()
            }) && meta.dev() != state_dev
        })
    };
    elsewhere(&paths.drafts, true)
        && elsewhere(&paths.claims, true)
        && elsewhere(&paths.summaries_lock, false)
}

fn loom_user_dir() -> Result<PathBuf> {
    #[cfg(test)]
    if std::env::var_os("LOOM_HOME")
        .filter(|value| !value.is_empty())
        .is_none()
    {
        return test_loom_user_dir();
    }

    let config = crate::user_config::config_path()?;
    config
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| anyhow!("loom config path has no parent: {}", config.display()))
}

/// Keep in-crate tests without an explicit home out of the operator's runtime state.
#[cfg(test)]
fn test_loom_user_dir() -> Result<PathBuf> {
    static HOME: std::sync::OnceLock<std::io::Result<tempfile::TempDir>> =
        std::sync::OnceLock::new();
    let home = HOME.get_or_init(|| tempfile::Builder::new().prefix("loom-test-home-").tempdir());
    match home {
        Ok(dir) => Ok(dir.path().to_path_buf()),
        Err(error) => Err(anyhow!("cannot create test loom user directory: {error}")),
    }
}

/// Runtime paths for the in-session CLI: honours `LOOM_PLAN_STATE` and `LOOM_PLAN_SESSION`.
pub fn runtime_paths(root: &Path) -> Result<RuntimePaths> {
    let state = std::env::var_os(STATE_VAR)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from);
    let session = std::env::var(SESSION_VAR).ok();
    runtime_paths_from(
        state.as_deref(),
        session.as_deref(),
        &loom_user_dir()?,
        root,
    )
}

/// Create the runtime directory mode 0700, and the loom user directory above it when that is
/// missing, refusing a symlink or a non-directory anywhere below the loom user directory.
pub fn create_runtime_dir(paths: &RuntimePaths) -> Result<()> {
    let base = paths
        .planning_root
        .parent()
        .context("the planning root has no parent directory")?;
    std::fs::create_dir_all(base).with_context(|| format!("cannot create {}", base.display()))?;
    let rel = paths
        .dir
        .strip_prefix(base)
        .context("the runtime directory is outside the loom user directory")?;
    safe_create_dir_all(base, rel, 0o700)?;
    let dir = File::from(safe_open_dirfd(&paths.dir)?);
    dir.set_permissions(Permissions::from_mode(0o700))
        .with_context(|| format!("cannot restrict {} to mode 0700", paths.dir.display()))
}

/// Open the runtime directory, creating it first when `create`. The server trusts the ancestors
/// of the loom user directory (`/tmp` may itself be a symlink) and refuses a symlink anywhere
/// below it, as [`create_runtime_dir`] does.
pub(crate) fn open_runtime_dir(runtime: &RuntimePaths, create: bool) -> Result<OwnedFd> {
    if create {
        create_runtime_dir(runtime)?;
    }
    let base = runtime
        .planning_root
        .parent()
        .context("the planning root has no parent directory")?;
    let relative = runtime
        .dir
        .strip_prefix(base)
        .context("the runtime directory is outside the loom user directory")?;
    let anchor = safe_open_dirfd(base)?;
    open_safely(
        anchor.as_raw_fd(),
        relative,
        libc::O_RDONLY | libc::O_DIRECTORY,
        0,
    )
    .with_context(|| format!("cannot open runtime directory {}", runtime.dir.display()))
}

/// Runtime paths for the server, which never honours `LOOM_PLAN_STATE`.
///
/// Returns the variable's value when it is set so the caller can warn that it was ignored.
pub fn server_runtime_paths(root: &Path) -> Result<(RuntimePaths, Option<OsString>)> {
    let ignored = std::env::var_os(STATE_VAR);
    let paths = runtime_paths_from(None, None, &loom_user_dir()?, root)?;
    Ok((paths, ignored))
}
