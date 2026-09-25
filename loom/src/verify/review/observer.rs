//! Which process computes a stage's change fingerprint (DESIGN D12, D13).
//!
//! The review gate compares the fingerprint a review round recorded with the
//! one computed at completion, and the test-integrity gate compares counts
//! derived from it with those a dispute accepted. Two processes computing the
//! two sides see different worktrees: a sandbox mounts `/dev/null` over some
//! root dotfiles (`verify::tool_artifacts`), masks git's global config and
//! with it `core.excludesFile`, and runs with another environment and `HOME`.
//! So one process computes every such value: the loom daemon of the project
//! that owns the worktree. Its own code computes directly
//! (`fingerprint::compute_local`); every other process asks it here, over
//! `.loom/work/orchestrator.sock`, with `Request::ObserveChanges`, which names
//! the stage and nothing else. The daemon resolves the worktree and target
//! branch itself (`daemon::server::observer`).
//!
//! This process computes a fingerprint itself only on positive evidence that
//! no daemon runs for the project: nothing answers on the socket
//! (`DaemonReach::NotListening`) AND the daemon's singleton lock is a regular
//! file this process can `flock` (`DaemonServer::proven_stopped`). A missing
//! socket alone proves nothing, because a sandbox can hide or deny the path
//! of one that exists; a missing lock proves nothing either, for the same
//! reason. Every other outcome is `DaemonUnreachable`, and the caller fails
//! closed. Computing locally, this process pins git to the stage's registered
//! git directory ([`WorktreeGit::pinned`]), so the worktree's `.git` file does
//! not choose the configuration git runs with.
//!
//! A stage session's sandbox denies `AF_UNIX`, so a sandboxed `loom stage
//! complete` cannot ask; it leaves both gates to the daemon, which runs them
//! when the completion broker asks it to apply the transition.

use anyhow::{bail, Context, Result};
use std::fmt;
use std::path::{Path, PathBuf};

use super::fingerprint::ChangeFingerprint;
use crate::daemon::{
    current_session_id, read_user_token, try_send_request, user_credential, DaemonReach,
    DaemonServer, Request, Response,
};
use crate::fs::work_dir::WorkDir;
use crate::git::worktree::{get_worktree_path, WorktreeGit};

/// Where a fingerprint comes from.
pub(super) enum Source {
    /// This process computes it, running git through the handle: `worktree`
    /// is no stage worktree, or no daemon runs for its project.
    ThisProcess(WorktreeGit),
    /// The owning daemon computed it against `target_branch`.
    Daemon {
        target_branch: String,
        fingerprint: ChangeFingerprint,
    },
}

/// No daemon answered for the worktree's project, and this process cannot
/// prove that none runs, so the fingerprint the daemon would compute cannot
/// be had here.
#[derive(Debug)]
pub(super) struct DaemonUnreachable {
    work_dir: PathBuf,
}

impl fmt::Display for DaemonUnreachable {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "no loom daemon answered on {}, and this process cannot prove that none runs (a \
             sandbox may deny unix sockets or hide the socket, and a running daemon holds \
             {}); the daemon computes every change fingerprint",
            self.work_dir.join("orchestrator.sock").display(),
            self.work_dir.join("orchestrator.lock").display()
        )
    }
}

impl std::error::Error for DaemonUnreachable {}

/// A stage worktree, its project and the project's state directory.
struct StageWorktree {
    project: PathBuf,
    worktree: PathBuf,
    work_dir: PathBuf,
    stage_id: String,
}

impl StageWorktree {
    fn git(&self) -> Result<WorktreeGit> {
        WorktreeGit::pinned(&self.project, &self.worktree)
    }
}

/// Where `worktree`'s fingerprint comes from: the owning daemon's answer, or
/// this process when `worktree` is no stage worktree or no daemon runs for
/// its project (module docs). Fails with `DaemonUnreachable` when neither is
/// established, and when the daemon does not answer with a fingerprint.
pub(super) fn source(worktree: &Path) -> Result<Source> {
    let Some(stage) = locate(worktree)? else {
        return Ok(Source::ThisProcess(WorktreeGit::discovered(worktree)));
    };
    let request = observe_request(&stage.work_dir, &stage.stage_id);
    match try_send_request(&stage.work_dir, &request)? {
        DaemonReach::NotListening if DaemonServer::proven_stopped(&stage.work_dir) => {
            stage.git().map(Source::ThisProcess)
        }
        DaemonReach::NotListening | DaemonReach::Unreachable => Err(DaemonUnreachable {
            work_dir: stage.work_dir,
        }
        .into()),
        DaemonReach::Answered(Response::ChangesObserved {
            target_branch,
            fingerprint,
        }) => Ok(Source::Daemon {
            target_branch,
            fingerprint,
        }),
        DaemonReach::Answered(Response::Error { message }) => bail!(
            "the loom daemon did not fingerprint stage '{}': {message}",
            stage.stage_id
        ),
        DaemonReach::Answered(other) => bail!(
            "the loom daemon refused to fingerprint stage '{}': {other:?}",
            stage.stage_id
        ),
    }
}

/// The git through which this process reads `worktree` itself: pinned to its
/// registered git directory when it is a stage worktree, whatever `.git`
/// in it says, and as discovered otherwise.
pub(crate) fn local_git(worktree: &Path) -> Result<WorktreeGit> {
    match locate(worktree)? {
        Some(stage) => stage.git(),
        None => Ok(WorktreeGit::discovered(worktree)),
    }
}

/// The project, state directory and stage of `worktree` when it is a stage
/// worktree, `<project>/.worktrees/<stage-id>` (loom names each stage's
/// worktree after the stage); `None` for any other directory, which no daemon
/// owns.
fn locate(worktree: &Path) -> Result<Option<StageWorktree>> {
    let worktree = worktree
        .canonicalize()
        .with_context(|| format!("cannot resolve the worktree {}", worktree.display()))?;
    let stage_id = worktree.file_name().and_then(|name| name.to_str());
    let project = worktree.parent().and_then(Path::parent);
    let (Some(stage_id), Some(project)) = (stage_id, project) else {
        return Ok(None);
    };
    let valid_id = crate::validation::validate_id(stage_id).is_ok();
    if !valid_id || get_worktree_path(stage_id, project) != worktree {
        return Ok(None);
    }
    let state = WorkDir::new(project)?;
    if state.repo_root() != Some(project) {
        return Ok(None);
    }
    Ok(Some(StageWorktree {
        project: project.to_path_buf(),
        work_dir: state.root().to_path_buf(),
        stage_id: stage_id.to_string(),
        worktree,
    }))
}

/// The request for `stage_id`'s fingerprint. A caller that can read the user
/// token runs on the host (a hook, the operator) and asks as the operator,
/// naming no session. One that cannot, a stage agent, names the session it
/// runs inside; the daemon proves that by peer identity and checks the
/// session owns the stage.
fn observe_request(work_dir: &Path, stage_id: &str) -> Request {
    let token = read_user_token(work_dir).filter(|token| !token.is_empty());
    let (auth_token, session_id) = match token {
        Some(token) => (token, String::new()),
        None => (user_credential(work_dir), current_session_id()),
    };
    Request::ObserveChanges {
        auth_token,
        stage_id: stage_id.to_string(),
        session_id,
    }
}
