//! Per-project planning runtime paths.
//!
//! Pure path computation: nothing here creates a file or directory. Outside a
//! planning session the runtime directory is `<loom user dir>/plan/<project key>`;
//! inside one, `LOOM_PLAN_STATE` names it and must pass a set of provenance checks.

use std::ffi::OsString;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use nix::unistd::{access, AccessFlags};

use crate::context::source_graph::body_hash;

/// Environment variable a planning session uses to name its runtime directory.
const STATE_VAR: &str = "LOOM_PLAN_STATE";
/// Environment variable carrying the planning session id.
const SESSION_VAR: &str = "LOOM_PLAN_SESSION";

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
            drafts: dir.join("drafts"),
            claims: dir.join("claims"),
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
    // The sandbox makes the real planning root read-only; a writable parent means the
    // caller made the directory itself and could bypass the server's lock and claims.
    if access(parent, AccessFlags::W_OK).is_ok() {
        bail!(
            "{STATE_VAR} parent {} is writable, so it is not the planning root",
            parent.display()
        );
    }
    Ok(parent.to_path_buf())
}

fn loom_user_dir() -> Result<PathBuf> {
    let config = crate::user_config::config_path()?;
    config
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| anyhow!("loom config path has no parent: {}", config.display()))
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

/// Runtime paths for the server, which never honours `LOOM_PLAN_STATE`.
///
/// Returns the variable's value when it is set so the caller can warn that it was ignored.
pub fn server_runtime_paths(root: &Path) -> Result<(RuntimePaths, Option<OsString>)> {
    let ignored = std::env::var_os(STATE_VAR);
    let paths = runtime_paths_from(None, None, &loom_user_dir()?, root)?;
    Ok((paths, ignored))
}
