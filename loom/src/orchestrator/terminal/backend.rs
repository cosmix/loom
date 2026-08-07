//! `SessionBackend`: dispatches session spawn/kill/liveness across the
//! native and tmux terminal backends according to the persisted `[terminal]`
//! config (`.work/config.toml`).
//!
//! # Fail-open
//!
//! Choosing `tmux` in `.work/config.toml` must never abort orchestration —
//! [`SessionBackend::from_config`] always succeeds as long as the config
//! itself reads cleanly, even when tmux is not installed. Availability is
//! resolved lazily, per spawn, by [`SessionBackend::resolve_lane`]: a
//! missing tmux, or a fallback already recorded from an earlier failure,
//! degrades to the native lane rather than erroring. This mirrors
//! `remote_control`'s `remote_control-unsupported` marker (see
//! [`crate::remote_control`]).
//!
//! # Fallback marker lifecycle
//!
//! `.work/terminal-backend-fallback` is written the first time a tmux spawn
//! fails, or the first time tmux is discovered unavailable. It lives in
//! `.work/` so it survives daemon restarts and separate `loom run`
//! invocations — nothing clears it automatically. The only clearing paths
//! are an explicit operator re-selection (`loom run --backend tmux`, see
//! [`clear_fallback_marker`]) and `loom clean --state` (which removes
//! `.work/` outright).

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::models::session::{Session, SessionBackendKind};
use crate::models::stage::Stage;
use crate::models::worktree::Worktree;

use super::native::NativeBackend;
use super::tmux::TmuxBackend;

/// Filename (under `.work/`) of the marker written after a tmux spawn
/// failure or unavailability, mirroring `remote_control-unsupported`.
const TERMINAL_BACKEND_FALLBACK_MARKER: &str = "terminal-backend-fallback";

fn fallback_marker_path(work_dir: &Path) -> PathBuf {
    work_dir.join(TERMINAL_BACKEND_FALLBACK_MARKER)
}

fn fallback_marker_exists(work_dir: &Path) -> bool {
    fallback_marker_path(work_dir).exists()
}

/// Best-effort write of the fallback marker. A write failure just means the
/// next spawn re-probes tmux availability — not a correctness gate.
fn write_fallback_marker(work_dir: &Path) {
    let _ = std::fs::write(
        fallback_marker_path(work_dir),
        "Terminal backend fell back to native after a tmux spawn failure or unavailable tmux.\n",
    );
}

/// Best-effort removal of the fallback marker, so a `tmux` lane can be
/// re-selected. Called by the CLI when an operator explicitly re-selects the
/// tmux backend (`loom run --backend tmux`).
pub fn clear_fallback_marker(work_dir: &Path) {
    let _ = std::fs::remove_file(fallback_marker_path(work_dir));
}

fn default_tmux_available() -> bool {
    which::which("tmux").is_ok()
}

pub struct SessionBackend {
    work_dir: PathBuf,
    /// The persisted `[terminal]` preference — NOT necessarily the lane a
    /// given spawn actually uses; see [`SessionBackend::resolve_lane`].
    configured_kind: SessionBackendKind,
    /// Eagerly constructed only when `configured_kind` is `Native` (today's
    /// behaviour, required so `Orchestrator::new`'s construction-failure
    /// semantics are unchanged). `None` when configured `Tmux`: the native
    /// lane is then built lazily, only if actually needed (tmux-spawn
    /// fallback, or kill/liveness of a native-recorded session).
    native: Option<NativeBackend>,
    tmux: TmuxBackend,
    /// Injectable tmux-availability probe, so lane resolution is unit
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
            tmux,
            tmux_available: default_tmux_available,
        })
    }

    /// The lane a spawn would actually use right now: `Native` when
    /// configured, or when `Tmux` is configured but unavailable (missing
    /// binary, or a previously recorded fallback); `Tmux` otherwise.
    ///
    /// `spawn_*` calls this itself rather than duplicating the decision, so
    /// tests asserting on the resolved lane are asserting on the same logic
    /// spawn uses.
    pub(crate) fn resolve_lane(&self) -> SessionBackendKind {
        if self.configured_kind == SessionBackendKind::Native {
            return SessionBackendKind::Native;
        }
        if fallback_marker_exists(&self.work_dir) {
            return SessionBackendKind::Native;
        }
        if (self.tmux_available)() {
            SessionBackendKind::Tmux
        } else {
            SessionBackendKind::Native
        }
    }

    pub fn backend_kind(&self) -> SessionBackendKind {
        self.configured_kind
    }

    /// Lazily construct a `NativeBackend` for the native lane. Only called
    /// when `self.native` is `None` (configured `Tmux`, falling back);
    /// construction is cheap — just terminal detection, no persistent
    /// resources — so a fresh instance per call is fine.
    fn native_for_spawn(&self) -> Result<NativeBackend> {
        NativeBackend::new(self.work_dir.clone())
    }

    fn spawn_native_lane(
        &self,
        mut session: Session,
        spawn_native: impl FnOnce(&NativeBackend, Session) -> Result<Session>,
    ) -> Result<Session> {
        session.backend = SessionBackendKind::Native;
        if let Some(native) = &self.native {
            return spawn_native(native, session);
        }
        let native = self.native_for_spawn()?;
        spawn_native(&native, session)
    }

    /// Shared spawn dispatcher: resolves the lane, stamps `session.backend`
    /// with the lane ACTUALLY used before delegating, and retries once on
    /// the native lane if a tmux-lane spawn fails.
    fn dispatch_spawn(
        &self,
        session: Session,
        spawn_native: impl FnOnce(&NativeBackend, Session) -> Result<Session>,
        spawn_tmux: impl FnOnce(&TmuxBackend, Session) -> Result<Session>,
    ) -> Result<Session> {
        let lane = self.resolve_lane();

        if lane == SessionBackendKind::Tmux {
            let mut tmux_session = session.clone();
            tmux_session.backend = SessionBackendKind::Tmux;
            match spawn_tmux(&self.tmux, tmux_session) {
                Ok(spawned) => return Ok(spawned),
                Err(err) => {
                    eprintln!(
                        "Warning: tmux backend spawn failed ({err}); retrying on the native lane and disabling tmux for this run."
                    );
                    write_fallback_marker(&self.work_dir);
                    // Fall through to the native retry below.
                }
            }
        } else if self.configured_kind == SessionBackendKind::Tmux
            && !fallback_marker_exists(&self.work_dir)
        {
            // Lane resolved to Native because tmux is unavailable, and this
            // is the first spawn to discover it — persist that so future
            // spawns (and future daemon restarts) skip re-probing tmux.
            eprintln!(
                "Warning: terminal backend \"tmux\" is configured but unavailable: install tmux, or set the [terminal] backend back to \"native\"."
            );
            write_fallback_marker(&self.work_dir);
        }

        self.spawn_native_lane(session, spawn_native)
    }

    pub fn spawn_session(
        &self,
        stage: &Stage,
        worktree: &Worktree,
        session: Session,
        signal_path: &Path,
    ) -> Result<Session> {
        self.dispatch_spawn(
            session,
            |native, s| native.spawn_session(stage, worktree, s, signal_path),
            |tmux, s| tmux.spawn_session(stage, worktree, s, signal_path),
        )
    }

    pub fn spawn_merge_session(
        &self,
        stage: &Stage,
        session: Session,
        signal_path: &Path,
        repo_root: &Path,
    ) -> Result<Session> {
        self.dispatch_spawn(
            session,
            |native, s| native.spawn_merge_session(stage, s, signal_path, repo_root),
            |tmux, s| tmux.spawn_merge_session(stage, s, signal_path, repo_root),
        )
    }

    pub fn spawn_knowledge_session(
        &self,
        stage: &Stage,
        session: Session,
        signal_path: &Path,
        repo_root: &Path,
    ) -> Result<Session> {
        self.dispatch_spawn(
            session,
            |native, s| native.spawn_knowledge_session(stage, s, signal_path, repo_root),
            |tmux, s| tmux.spawn_knowledge_session(stage, s, signal_path, repo_root),
        )
    }

    /// Dispatches on `session.backend` (the lane it actually spawned on),
    /// NOT the currently configured kind — a session recorded as `Native`
    /// must be killed via the native lane even if the config now says tmux.
    pub fn kill_session(&self, session: &Session) -> Result<()> {
        match session.backend {
            SessionBackendKind::Tmux => self.tmux.kill_session(session),
            SessionBackendKind::Native => {
                if let Some(native) = &self.native {
                    return native.kill_session(session);
                }
                if let Ok(native) = self.native_for_spawn() {
                    return native.kill_session(session);
                }
                // No terminal detected (e.g. headless): degrade to the
                // PID-only teardown rather than erroring.
                self.kill_session_pid_only(session)
            }
        }
    }

    fn kill_session_pid_only(&self, session: &Session) -> Result<()> {
        let resolved = NativeBackend::window_title_and_pid_key(session);
        let pid_to_kill = match resolved.as_ref() {
            Some((_, pid_key)) => match super::native::read_pid_entry(&self.work_dir, pid_key) {
                Some(entry) if super::native::pid_matches_entry(&entry) => Some(entry.pid),
                Some(_) => None,
                None => session.pid,
            },
            None => session.pid,
        };

        if let Some(pid) = pid_to_kill {
            crate::process::terminate(pid)
                .with_context(|| format!("Failed to terminate session process {pid}"))?;
        }

        if let Some((_, pid_key)) = &resolved {
            super::native::cleanup_stage_files(&self.work_dir, pid_key);
        }

        Ok(())
    }

    /// Dispatches on `session.backend`, same reasoning as `kill_session`.
    pub fn is_session_alive(&self, session: &Session) -> Result<bool> {
        match session.backend {
            SessionBackendKind::Tmux => self.tmux.is_session_alive(session),
            SessionBackendKind::Native => {
                if let Some(native) = &self.native {
                    return native.is_session_alive(session);
                }
                if let Ok(native) = self.native_for_spawn() {
                    return native.is_session_alive(session);
                }
                Ok(self.is_session_alive_pid_only(session))
            }
        }
    }

    fn is_session_alive_pid_only(&self, session: &Session) -> bool {
        let resolved = NativeBackend::window_title_and_pid_key(session);
        if let Some((_, pid_key)) = &resolved {
            if let Some(entry) = super::native::read_pid_entry(&self.work_dir, pid_key) {
                if super::native::pid_matches_entry(&entry) {
                    return true;
                }
                super::native::cleanup_stage_files(&self.work_dir, pid_key);
            }
        }
        if let Some(pid) = session.pid {
            if crate::process::is_process_alive(pid) {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_backend(
        work_dir: PathBuf,
        configured_kind: SessionBackendKind,
        tmux_available: fn() -> bool,
    ) -> SessionBackend {
        SessionBackend {
            tmux: TmuxBackend::new(work_dir.clone()),
            work_dir,
            configured_kind,
            native: None,
            tmux_available,
        }
    }

    #[test]
    fn missing_tmux_falls_back_to_native_lane() {
        let temp = TempDir::new().unwrap();
        let backend = test_backend(temp.path().to_path_buf(), SessionBackendKind::Tmux, || {
            false
        });
        assert_eq!(backend.resolve_lane(), SessionBackendKind::Native);
    }

    #[test]
    fn fallback_marker_forces_native_lane() {
        let temp = TempDir::new().unwrap();
        write_fallback_marker(temp.path());
        let backend = test_backend(temp.path().to_path_buf(), SessionBackendKind::Tmux, || true);
        assert_eq!(backend.resolve_lane(), SessionBackendKind::Native);
    }

    #[test]
    fn tmux_configured_available_no_marker_resolves_tmux_lane() {
        let temp = TempDir::new().unwrap();
        let backend = test_backend(temp.path().to_path_buf(), SessionBackendKind::Tmux, || true);
        assert_eq!(backend.resolve_lane(), SessionBackendKind::Tmux);
    }

    #[test]
    fn native_configured_always_resolves_native_lane_even_if_tmux_available() {
        let temp = TempDir::new().unwrap();
        let backend = test_backend(
            temp.path().to_path_buf(),
            SessionBackendKind::Native,
            || true,
        );
        assert_eq!(backend.resolve_lane(), SessionBackendKind::Native);
    }

    #[test]
    fn clear_fallback_marker_removes_it() {
        let temp = TempDir::new().unwrap();
        write_fallback_marker(temp.path());
        assert!(fallback_marker_exists(temp.path()));
        clear_fallback_marker(temp.path());
        assert!(!fallback_marker_exists(temp.path()));
    }
}
