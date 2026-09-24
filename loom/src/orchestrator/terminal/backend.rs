//! `SessionBackend`: dispatches session spawn/kill/liveness across the
//! native and tmux terminal backends according to the resolved `[terminal]`
//! config (project `.loom/work/config.toml`, then `~/.loom/config.toml`, then
//! the built-in default — see [`crate::fs::work_dir::read_terminal_config`]).
//!
//! The configured backend is authoritative and nothing on disk can override
//! it: choosing `tmux` with no `tmux` on PATH is a hard spawn-time error
//! naming the fix, never a silent switch to the native lane.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::models::session::{Session, SessionBackendKind, SessionType};
use crate::models::stage::Stage;
use crate::models::worktree::Worktree;

use super::native::NativeBackend;
use super::tmux::TmuxBackend;

fn default_tmux_available() -> bool {
    which::which("tmux").is_ok()
}

pub struct SessionBackend {
    work_dir: PathBuf,
    /// The resolved `[terminal]` preference — the lane every spawn uses.
    configured_kind: SessionBackendKind,
    /// Eagerly constructed only when `configured_kind` is `Native` (today's
    /// behaviour, required so `Orchestrator::new`'s construction-failure
    /// semantics are unchanged). `None` when configured `Tmux`: the native
    /// lane is then built lazily, only if actually needed (kill/liveness of a
    /// session recorded `backend = native` from an earlier configuration).
    native: Option<NativeBackend>,
    /// Memoized lazy native lane, used only when `native` is `None`. See
    /// [`SessionBackend::native_lane`] for why the FAILURE is memoized too.
    lazy_native: OnceLock<std::result::Result<NativeBackend, String>>,
    tmux: TmuxBackend,
    /// Injectable tmux-availability probe, so the spawn-time check is unit
    /// testable without depending on the host actually having tmux.
    tmux_available: fn() -> bool,
}

impl SessionBackend {
    pub fn from_config(work_dir: PathBuf) -> Result<Self> {
        let config = crate::fs::work_dir::read_terminal_config(&work_dir)
            .context("Failed to read [terminal] config")?;
        let native = match config.backend {
            SessionBackendKind::Native => Some(NativeBackend::new(work_dir.clone())?),
            SessionBackendKind::Tmux => None,
        };
        let tmux = TmuxBackend::new(work_dir.clone());
        Ok(Self {
            work_dir,
            configured_kind: config.backend,
            native,
            lazy_native: OnceLock::new(),
            tmux,
            tmux_available: default_tmux_available,
        })
    }

    /// The lane every spawn uses: the resolved `[terminal]` config, verbatim.
    ///
    /// Kept as its own method (rather than reading `configured_kind`
    /// directly) so the write-ahead session record — stamped with this lane
    /// before a spawn is attempted, see `session_lifecycle::write_ahead_session`
    /// — and the spawn dispatcher below can never read two different answers.
    pub(crate) fn resolve_lane(&self) -> SessionBackendKind {
        self.configured_kind
    }

    /// The native lane, constructed AT MOST ONCE per `SessionBackend`.
    ///
    /// `NativeBackend::new` runs terminal detection, which shells out to
    /// `which`/`gsettings`/AppleScript probes. For a session recorded
    /// `backend = native` under a tmux-configured `SessionBackend` (spawned
    /// before the config was switched to tmux), [`Self::is_session_alive`]
    /// reaches this path once per session on every 5-second monitor tick, so
    /// rebuilding per call meant a burst of subprocesses (and, before this, a
    /// line of stderr) every tick forever.
    ///
    /// The FAILURE is memoized alongside the success: terminal availability is
    /// a property of the daemon's environment, fixed for the life of the
    /// process, and re-probing it thousands of times to get the same "no
    /// terminal" answer buys nothing. It is stored as a `String` because
    /// `anyhow::Error` is not `Clone` and callers only ever surface it as text.
    ///
    /// `OnceLock` rather than `RefCell`/`Mutex`: the orchestrator holds this
    /// behind an `Arc` and calls it from the monitor thread, so it must stay
    /// `Send + Sync` and must not require `&mut self`.
    fn native_lane(&self) -> std::result::Result<&NativeBackend, &str> {
        if let Some(native) = &self.native {
            return Ok(native);
        }
        self.lazy_native
            .get_or_init(|| {
                NativeBackend::new(self.work_dir.clone()).map_err(|err| format!("{err:#}"))
            })
            .as_ref()
            .map_err(String::as_str)
    }

    fn spawn_native_lane(
        &self,
        mut session: Session,
        spawn_native: impl FnOnce(&NativeBackend, Session) -> Result<Session>,
    ) -> Result<Session> {
        session.backend = SessionBackendKind::Native;
        let native = self.native_lane().map_err(|err| anyhow::anyhow!("{err}"))?;
        spawn_native(native, session)
    }

    /// Shared spawn dispatcher: dispatches on the CONFIGURED lane only — no
    /// availability probe ever swaps it for the other lane. A configured tmux
    /// backend with no `tmux` on PATH, or whose spawn fails, returns `Err`
    /// straight to the caller; nothing is written to disk and no native retry
    /// is attempted.
    fn dispatch_spawn(
        &self,
        session: Session,
        spawn_native: impl FnOnce(&NativeBackend, Session) -> Result<Session>,
        spawn_tmux: impl FnOnce(&TmuxBackend, Session) -> Result<Session>,
    ) -> Result<Session> {
        match self.configured_kind {
            SessionBackendKind::Native => self.spawn_native_lane(session, spawn_native),
            SessionBackendKind::Tmux => {
                if !(self.tmux_available)() {
                    anyhow::bail!(
                        "terminal backend \"tmux\" is configured but tmux is not on PATH; \
                         install tmux or set [terminal] backend = \"native\" \
                         (.loom/work/config.toml or ~/.loom/config.toml)"
                    );
                }
                let mut tmux_session = session;
                tmux_session.backend = SessionBackendKind::Tmux;
                let session_id = tmux_session.id.clone();
                spawn_tmux(&self.tmux, tmux_session)
                    .with_context(|| format!("tmux spawn failed for session '{session_id}'"))
            }
        }
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

    /// Lane dispatch for every session kind that runs in the stage worktree.
    fn spawn_worktree_session(
        &self,
        kind: SessionType,
        stage: &Stage,
        worktree: &Worktree,
        session: Session,
        signal_path: &Path,
    ) -> Result<Session> {
        self.dispatch_spawn(
            session,
            |native, s| native.spawn_session(kind, stage, worktree, s, signal_path),
            |tmux, s| tmux.spawn_session(kind, stage, worktree, s, signal_path),
        )
    }

    pub fn spawn_merge_session(
        &self,
        stage: &Stage,
        session: Session,
        signal_path: &Path,
        repo_root: &Path,
    ) -> Result<Session> {
        self.spawn_main_repo_session(SessionType::Merge, stage, session, signal_path, repo_root)
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

    /// Spawn the session that judges one disputed acceptance criterion.
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

    /// Lane dispatch for every session kind that runs in the main repository.
    fn spawn_main_repo_session(
        &self,
        kind: SessionType,
        stage: &Stage,
        session: Session,
        signal_path: &Path,
        repo_root: &Path,
    ) -> Result<Session> {
        self.dispatch_spawn(
            session,
            |native, s| native.spawn_main_repo_session(kind, stage, s, signal_path, repo_root),
            |tmux, s| tmux.spawn_main_repo_session(kind, stage, s, signal_path, repo_root),
        )
    }

    /// Dispatches on `session.backend` (the lane it actually spawned on),
    /// NOT the currently configured kind — a session recorded as `Native`
    /// must be killed via the native lane even if the config now says tmux.
    pub fn kill_session(&self, session: &Session) -> Result<()> {
        match session.backend {
            SessionBackendKind::Tmux => self.tmux.kill_session(session),
            SessionBackendKind::Native => match self.native_lane() {
                Ok(native) => native.kill_session(session),
                // No terminal detected (e.g. headless): degrade to the shared
                // PID-only teardown rather than erroring. The window-close
                // attempt is the only thing lost, and without a terminal there
                // is no window to close.
                Err(_) => super::native::pid_only_terminate(&self.work_dir, session),
            },
        }
    }

    /// Dispatches on `session.backend`, same reasoning as `kill_session`.
    pub fn is_session_alive(&self, session: &Session) -> Result<bool> {
        match session.backend {
            SessionBackendKind::Tmux => self.tmux.is_session_alive(session),
            SessionBackendKind::Native => match self.native_lane() {
                Ok(native) => native.is_session_alive(session),
                // Headless: only the window-existence layer is unavailable,
                // and the PID layers are the authoritative ones anyway.
                Err(_) => Ok(super::native::pid_only_is_alive(&self.work_dir, session)),
            },
        }
    }
}

#[cfg(test)]
mod tests;
