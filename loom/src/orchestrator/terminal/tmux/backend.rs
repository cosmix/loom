//! The `TmuxBackend` type, split out of `tmux/mod.rs` to keep that module
//! under the 400-line ceiling (CLAUDE.md Rule 17).
//!
//! The free functions this drives — the argv builder, the spawn, the abort and
//! teardown helpers — stay in the parent module, which owns the tmux protocol
//! itself. This file owns only the backend's session lifecycle.

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::models::session::{Session, SessionType};
use crate::models::stage::Stage;
use crate::models::worktree::Worktree;

use super::native;
use super::{
    abort_tmux_spawn, await_tmux_session_pid, socket_name, spawn_in_tmux, teardown_socket,
};

/// Tmux terminal backend — spawns sessions in headless tmux servers.
pub struct TmuxBackend {
    /// The `.work` directory path, for PID tracking and session lookups.
    work_dir: PathBuf,
}

impl TmuxBackend {
    /// Create a new tmux backend. Unlike [`native::NativeBackend::new`], this
    /// never fails: it does not probe for tmux or any terminal — that check
    /// happens at spawn time (see `SessionBackend::resolve_lane`).
    pub fn new(work_dir: PathBuf) -> Self {
        Self { work_dir }
    }

    /// Unified spawn for every tmux session type, mirroring
    /// `NativeBackend::spawn`.
    ///
    /// # Every post-preparation failure MUST clean up
    ///
    /// Once `prepare_session_launch` returns, a wrapper script and a PID-file
    /// slot exist; once `spawn_in_tmux` starts talking to tmux, a server and a
    /// real claude process may exist even on the failure paths — `has-session`
    /// can fail after `new-session` created the session, and
    /// `evaluate_new_session` rejects an exit-0 spawn whose stderr carried
    /// nothing worse than a `~/.tmux.conf` warning.
    ///
    /// Returning without tearing that down is not a leak of a stray process,
    /// it is a CORRECTNESS bug: `SessionBackend::dispatch_spawn` retries on the
    /// native lane, so the stage ends up with TWO claude agents editing the
    /// same worktree. The stale PID file is the second half of the same bug —
    /// `await_session_pid` returns the first live PID it reads under this
    /// session's `pid_key`, which would be the orphaned tmux claude, so the
    /// native retry would adopt it while stamping `backend = Native`.
    ///
    /// Hence one `abort` closure (`abort_tmux_spawn`), used by every error
    /// path below.
    fn spawn(
        &self,
        kind: SessionType,
        stage: &Stage,
        session: Session,
        signal_path: &Path,
        cwd: &Path,
        set_worktree_path: bool,
    ) -> Result<Session> {
        let (mut session, title, pid_key, wrapper_path) =
            native::prepare_session_launch(&self.work_dir, kind, stage, session, signal_path, cwd)?;

        let socket = socket_name(&session);
        let abort = || abort_tmux_spawn(&self.work_dir, &socket, &pid_key);

        if let Err(err) = spawn_in_tmux(&socket, &title, cwd, &wrapper_path) {
            abort();
            return Err(err);
        }

        let pid = match await_tmux_session_pid(&self.work_dir, &pid_key, cwd, &session.id) {
            Ok(pid) => pid,
            Err(err) => {
                abort();
                return Err(err);
            }
        };

        if set_worktree_path {
            session.set_worktree_path(cwd.to_path_buf());
        }
        session.set_pid(pid);
        if let Err(err) = session.try_mark_running() {
            abort();
            return Err(err);
        }

        Ok(session)
    }

    pub fn spawn_session(
        &self,
        stage: &Stage,
        worktree: &Worktree,
        session: Session,
        signal_path: &Path,
    ) -> Result<Session> {
        self.spawn(
            SessionType::Stage,
            stage,
            session,
            signal_path,
            &worktree.path,
            true,
        )
    }

    pub fn spawn_merge_session(
        &self,
        stage: &Stage,
        session: Session,
        signal_path: &Path,
        repo_root: &Path,
    ) -> Result<Session> {
        self.spawn(
            SessionType::Merge,
            stage,
            session,
            signal_path,
            repo_root,
            false,
        )
    }

    pub fn spawn_knowledge_session(
        &self,
        stage: &Stage,
        session: Session,
        signal_path: &Path,
        repo_root: &Path,
    ) -> Result<Session> {
        self.spawn(
            SessionType::Knowledge,
            stage,
            session,
            signal_path,
            repo_root,
            false,
        )
    }

    pub fn kill_session(&self, session: &Session) -> Result<()> {
        // First use the guarded PID branch shared with the native lane: only
        // signal when PID and start-time both match. `session.pid` is never a
        // destructive fallback. A failed graceful signal must not skip socket
        // teardown, which is positively attributed by this session's ID.
        if native::session_process_status(&self.work_dir, session)
            == native::SessionProcessStatus::VerifiedAlive
        {
            let _ = native::pid_only_terminate(&self.work_dir, session);
        }

        // Then tear down this session's tmux server. This is the one place
        // that knows the exact socket path at clean-teardown time.
        teardown_socket(&socket_name(session));
        Ok(())
    }

    /// Whether `session` is alive, using ONLY the PID layers — never
    /// `tmux has-session`.
    ///
    /// A server whose pane process has died but which has not yet reaped
    /// itself still answers `has-session` with exit 0. Using that as a
    /// liveness source would make the monitor report a dead claude as ALIVE,
    /// never file the crash, and never retry — defeating the containment
    /// property this backend exists to deliver. PID-file evidence (with
    /// start-time verification against reuse) is the only source of truth,
    /// which is precisely what `native::pid_only_is_alive` implements; this
    /// lane simply adds NOTHING on top of it.
    pub fn is_session_alive(&self, session: &Session) -> Result<bool> {
        Ok(native::pid_only_is_alive(&self.work_dir, session))
    }
}
