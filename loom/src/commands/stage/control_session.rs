//! Routing for the two non-ordinary `loom stage complete` paths.
//!
//! A stage session that runs inside a loom worktree is sandboxed: it may run
//! acceptance and verification, but the state transition itself belongs to the
//! daemon. Two pieces implement that split, and both live here because both
//! decide *identity* from the wrapper environment:
//!
//! * [`sandbox_control_session`] — called on every completion, decides whether
//!   this invocation is a sandboxed worktree agent (verification only) or an
//!   ordinary host-side completion.
//! * [`handle_broker_request`] — the `LOOM_CONTROL_BROKER=1` re-entry made by
//!   `hooks/loom-control-complete.sh` after it sees the verification marker,
//!   which forwards the transition to the daemon over the socket.

use super::control_complete;
use crate::daemon::DaemonServer;
use crate::models::stage::Stage;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

/// Serve a `LOOM_CONTROL_BROKER=1` invocation, returning whether it was
/// handled (in which case the caller must not continue into the ordinary
/// completion pipeline).
pub(super) fn handle_broker_request(
    stage_id: &str,
    session_id: Option<&str>,
    has_privileged_flags: bool,
    work_dir: &Path,
) -> Result<bool> {
    if !control_complete::broker_requested() {
        return Ok(false);
    }
    if has_privileged_flags {
        bail!("trusted completion broker does not accept privileged flags");
    }
    let session_id = session_id.context("trusted completion broker requires --session")?;
    require_wrapper_identity(stage_id, session_id)?;
    control_complete::send_completion(stage_id, session_id, work_dir)?;
    Ok(true)
}

fn require_wrapper_identity(stage_id: &str, session_id: &str) -> Result<()> {
    let env_stage = std::env::var("LOOM_STAGE_ID").context("LOOM_STAGE_ID is missing")?;
    let env_session = std::env::var("LOOM_SESSION_ID").context("LOOM_SESSION_ID is missing")?;
    if env_stage != stage_id || env_session != session_id {
        bail!("trusted completion broker identity does not match wrapper identity");
    }
    Ok(())
}

/// Whether `path` names a loom worktree — `<repo>/.worktrees/<stage-id>`.
///
/// Purely structural (no filesystem access) because it decides *routing*, not
/// authorization: [`sandbox_control_session`] still canonicalizes the path and
/// requires the working directory to sit inside it. Requiring a component
/// after `.worktrees` keeps the bare container directory out.
pub(super) fn is_loom_worktree_path(path: &Path) -> bool {
    let mut components = path.components();
    while let Some(component) = components.next() {
        if component.as_os_str() == ".worktrees" {
            return components.next().is_some();
        }
    }
    false
}

/// The session id this completion is acting for, when it is a sandboxed
/// worktree agent whose only authority is verification. `None` means an
/// ordinary host-side completion.
pub(super) fn sandbox_control_session(
    stage: &Stage,
    stage_id: &str,
    requested_session: Option<&str>,
    work_dir: &Path,
) -> Result<Option<String>> {
    let (Ok(env_stage), Ok(env_session), Ok(worktree)) = (
        std::env::var("LOOM_STAGE_ID"),
        std::env::var("LOOM_SESSION_ID"),
        std::env::var("LOOM_WORKTREE_PATH"),
    ) else {
        return Ok(None);
    };
    // Membership, not presence. A loom worktree lives at
    // `<repo>/.worktrees/<stage-id>/`; the main repo root does not. Sessions
    // that run in the main repo (knowledge, merge, base-conflict) have no
    // worktree and complete through the ordinary in-process path, so a bare
    // "the variable is set" test would route them into a sandboxed wrapper
    // route that cannot serve them. Mirrors `loom_current_worktree()` in
    // `hooks/_common.sh`, which has always required this.
    if !is_loom_worktree_path(Path::new(&worktree)) {
        return Ok(None);
    }
    if env_stage != stage_id || stage.session.as_deref() != Some(env_session.as_str()) {
        bail!("completion request does not match the active wrapper stage/session");
    }
    if requested_session.is_some_and(|requested| requested != env_session) {
        bail!("--session does not match the active wrapper session");
    }
    let cwd = std::env::current_dir().context("failed to resolve completion working directory")?;
    let worktree = PathBuf::from(worktree)
        .canonicalize()
        .context("failed to resolve LOOM_WORKTREE_PATH")?;
    let cwd = cwd
        .canonicalize()
        .context("failed to resolve current directory")?;
    if !cwd.starts_with(&worktree) {
        bail!("wrapper completion must run inside its assigned worktree");
    }
    if !DaemonServer::is_running(work_dir) {
        bail!("sandboxed worktree completion requires the loom daemon to be running");
    }
    Ok(Some(env_session))
}

#[cfg(test)]
mod tests {
    use super::is_loom_worktree_path;
    use std::path::Path;

    /// The sandboxed-completion route must be selected by WORKTREE MEMBERSHIP,
    /// not by `LOOM_WORKTREE_PATH` merely being set.
    ///
    /// The wrapper script used to export that variable for every session kind,
    /// including knowledge / merge / base-conflict sessions that run in the
    /// main repo. `sandbox_control_session` read bare presence as "this is a
    /// sandboxed worktree agent", so a knowledge stage was routed into a
    /// wrapper path that explicitly refuses knowledge stages — leaving it
    /// permanently unable to complete itself even with every acceptance
    /// criterion green.
    #[test]
    fn worktree_membership_is_structural_not_presence() {
        // Real loom worktrees.
        assert!(is_loom_worktree_path(Path::new(
            "/home/dev/repo/.worktrees/build-api"
        )));
        assert!(is_loom_worktree_path(Path::new(
            "/home/dev/repo/.worktrees/build-api/src/nested"
        )));

        // Main-repo session working directories — knowledge, merge and
        // base-conflict sessions all `cd` here.
        assert!(!is_loom_worktree_path(Path::new("/home/dev/repo")));
        assert!(!is_loom_worktree_path(Path::new("/")));

        // The bare container directory is not itself a worktree.
        assert!(!is_loom_worktree_path(Path::new(
            "/home/dev/repo/.worktrees"
        )));

        // A directory that merely mentions the name is not one either.
        assert!(!is_loom_worktree_path(Path::new(
            "/home/dev/repo/my.worktrees-backup"
        )));
    }
}
