//! `SessionBackend`: the facade every orchestrator path uses to spawn, kill
//! and probe sessions. Every session runs in tmux; a missing `tmux` on PATH is
//! a hard spawn-time error naming the fix, checked before any launch file is
//! written.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::git::branch::branch_name_for_stage;
use crate::models::session::{Session, SessionType};
use crate::models::stage::Stage;
use crate::models::worktree::Worktree;

use super::tmux::{self, TmuxBackend};

/// Why a session cannot start without `tmux` on PATH: the `loom run`
/// preflight, `loom init` and the spawn dispatcher all report it.
pub const TMUX_MISSING_MESSAGE: &str =
    "Loom needs tmux on PATH to start sessions. Install tmux and retry.";

/// Fail with [`TMUX_MISSING_MESSAGE`] when `tmux_available` reports no tmux.
pub fn require_tmux(tmux_available: impl Fn() -> bool) -> Result<()> {
    if tmux_available() {
        Ok(())
    } else {
        Err(anyhow::anyhow!(TMUX_MISSING_MESSAGE))
    }
}

/// The stage worktree a merge resolver works in. Errors when its directory
/// is missing: the caller routes the stage to human review.
pub fn merge_resolver_worktree(repo_root: &Path, stage_id: &str) -> Result<Worktree> {
    let path = Worktree::worktree_path(repo_root, stage_id);
    if !path.is_dir() {
        anyhow::bail!(
            "the stage worktree {} is missing; the merge resolver works there",
            path.display()
        );
    }
    Ok(Worktree::new(
        stage_id.to_string(),
        path,
        branch_name_for_stage(stage_id),
    ))
}

/// Whether `tmux` is on PATH. Every session loom starts runs in tmux, so the
/// run entry points and `loom init <plan>` all probe it through this one fn.
pub fn tmux_on_path() -> bool {
    which::which("tmux").is_ok()
}

/// Holds no work dir of its own: `TmuxBackend` owns it, and nothing here reads it.
pub struct SessionBackend {
    tmux: TmuxBackend,
    /// Injectable tmux-availability probe, so the spawn-time check is unit
    /// testable without depending on the host actually having tmux.
    tmux_available: fn() -> bool,
}

impl SessionBackend {
    pub fn new(work_dir: PathBuf) -> Self {
        Self::build(work_dir, tmux_on_path)
    }

    /// A backend whose tmux-availability probe is `tmux_available`.
    #[cfg(test)]
    pub(crate) fn with_tmux_probe(work_dir: PathBuf, tmux_available: fn() -> bool) -> Self {
        Self::build(work_dir, tmux_available)
    }

    fn build(work_dir: PathBuf, tmux_available: fn() -> bool) -> Self {
        Self {
            tmux: TmuxBackend::new(work_dir),
            tmux_available,
        }
    }

    /// Fail with [`TMUX_MISSING_MESSAGE`] when tmux is not on PATH. Callers
    /// that publish launch state before reaching a spawn method run it first.
    pub fn ensure_tmux(&self) -> Result<()> {
        require_tmux(self.tmux_available)
    }

    /// The one spawn seam: checks for tmux before `spawn_tmux` can write a
    /// wrapper or PID file. A failure after launch preparation may leave
    /// generated wrapper or settings files for the normal cleanup path.
    fn dispatch_spawn(
        &self,
        session: Session,
        spawn_tmux: impl FnOnce(&TmuxBackend, Session) -> Result<Session>,
    ) -> Result<Session> {
        self.ensure_tmux()?;
        let session_id = session.id.clone();
        spawn_tmux(&self.tmux, session)
            .with_context(|| format!("tmux spawn failed for session '{session_id}'"))
    }

    pub fn spawn_session(
        &self,
        stage: &Stage,
        worktree: &Worktree,
        session: Session,
        signal_path: &Path,
    ) -> Result<Session> {
        self.spawn_worktree_session(SessionType::Stage, stage, worktree, session, signal_path)
    }

    /// Spawn the agent that writes a v2 stage's contract tests. It runs in
    /// the stage worktree, exactly where the `Stage` session that follows it
    /// will run.
    pub fn spawn_contract_session(
        &self,
        stage: &Stage,
        worktree: &Worktree,
        session: Session,
        signal_path: &Path,
    ) -> Result<Session> {
        self.spawn_worktree_session(SessionType::Contract, stage, worktree, session, signal_path)
    }

    /// Spawn the merge conflict resolver. It runs in the stage worktree and
    /// merges the target branch into the stage branch there.
    pub fn spawn_merge_session_in_worktree(
        &self,
        stage: &Stage,
        worktree: &Worktree,
        session: Session,
        signal_path: &Path,
    ) -> Result<Session> {
        self.spawn_worktree_session(SessionType::Merge, stage, worktree, session, signal_path)
    }

    /// Spawn every session kind that runs in the stage worktree.
    fn spawn_worktree_session(
        &self,
        kind: SessionType,
        stage: &Stage,
        worktree: &Worktree,
        session: Session,
        signal_path: &Path,
    ) -> Result<Session> {
        self.dispatch_spawn(session, |tmux, s| {
            tmux.spawn_session(kind, stage, worktree, s, signal_path)
        })
    }

    pub fn spawn_knowledge_session(
        &self,
        stage: &Stage,
        session: Session,
        signal_path: &Path,
        repo_root: &Path,
    ) -> Result<Session> {
        self.spawn_main_repo_session(
            SessionType::Knowledge,
            stage,
            session,
            signal_path,
            repo_root,
        )
    }

    /// Spawn the session that judges one disputed criterion
    /// (acceptance criterion, wiring check, or wiring test).
    ///
    /// It never gets a worktree of its own: `repo_root` is whatever
    /// `judge_cwd` (`orchestrator/adjudication/session.rs`) picked — the
    /// disputed stage's own worktree while it still exists, so a criterion
    /// like `cargo test` runs where it wrote its output, else the main
    /// repository.
    pub fn spawn_adjudication_session(
        &self,
        stage: &Stage,
        session: Session,
        signal_path: &Path,
        repo_root: &Path,
    ) -> Result<Session> {
        self.spawn_main_repo_session(
            SessionType::Adjudication,
            stage,
            session,
            signal_path,
            repo_root,
        )
    }

    /// Spawn every session kind that runs in the main repository
    /// (knowledge and adjudication).
    fn spawn_main_repo_session(
        &self,
        kind: SessionType,
        stage: &Stage,
        session: Session,
        signal_path: &Path,
        repo_root: &Path,
    ) -> Result<Session> {
        self.dispatch_spawn(session, |tmux, s| {
            tmux.spawn_main_repo_session(kind, stage, s, signal_path, repo_root)
        })
    }

    pub fn kill_session(&self, session: &Session) -> Result<()> {
        self.tmux.kill_session(session)
    }

    /// Release what a session whose agent has already exited still holds.
    ///
    /// A tmux session's server outlives the agent (`remain-on-exit` keeps the
    /// dead pane so its history can be saved), so an exit nobody kills would
    /// leave the server running for good. This ends it through
    /// [`Self::kill_session`], which saves the history first, and tries again
    /// once when that fails.
    pub fn release_ended_session(&self, session: &Session) -> Result<()> {
        end_tmux_server(session, || self.tmux.kill_session(session))
    }

    pub fn is_session_alive(&self, session: &Session) -> Result<bool> {
        self.tmux.is_session_alive(session)
    }
}

/// Run `end_server` and, when it fails, once more: a `kill-server` that times
/// out under load usually succeeds on the next try. The caller records the
/// session as ended whatever happens here, so a server that survives both
/// attempts stays until `loom clean`; every failure names the session and the
/// socket, so that server can be found.
fn end_tmux_server(session: &Session, mut end_server: impl FnMut() -> Result<()>) -> Result<()> {
    let Err(first) = end_server() else {
        return Ok(());
    };
    let socket = tmux::socket_name(session);
    tracing::warn!(
        session_id = %session.id,
        %socket,
        error = %format!("{first:#}"),
        "Could not end the tmux server of an ended session; retrying once"
    );
    end_server().map_err(|second| {
        anyhow::anyhow!(
            "the tmux server on socket {socket} of ended session {} is still running \
             (`loom clean` ends it): {second:#}",
            session.id
        )
    })
}

#[cfg(test)]
mod tests;
