//! `SessionBackend`: dispatches session spawn/kill/liveness across the
//! native and tmux terminal backends according to the persisted `[terminal]`
//! config (`.work/config.toml`).
//!
//! # Fail-open
//!
//! Choosing `tmux` in `.work/config.toml` must never abort orchestration —
//! [`SessionBackend::from_config`] always succeeds as long as the config
//! itself reads cleanly, even when tmux is not installed. Availability is
//! resolved lazily, per spawn, by `SessionBackend::resolve_lane`: a
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
//!
//! Because it is that sticky, it is written ONLY once the native lane is
//! known to be constructible. On a headless host `NativeBackend::new` bails
//! in terminal detection, so a marker written there would permanently
//! disable tmux — for every later spawn and every later daemon start — in
//! exchange for a retry that could not have succeeded.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

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
    /// Memoized lazy native lane, used only when `native` is `None`. See
    /// [`SessionBackend::native_lane`] for why the FAILURE is memoized too.
    lazy_native: OnceLock<std::result::Result<NativeBackend, String>>,
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
            lazy_native: OnceLock::new(),
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

    /// The native lane, constructed AT MOST ONCE per `SessionBackend`.
    ///
    /// `NativeBackend::new` runs terminal detection, which shells out to
    /// `which`/`gsettings`/AppleScript probes. After a tmux fallback,
    /// [`Self::is_session_alive`] reaches this path once per native session on
    /// every 5-second monitor tick, so rebuilding per call meant a burst of
    /// subprocesses (and, before this, a line of stderr) every tick forever.
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

    /// Handle a failed tmux-lane spawn: either arm the native retry, or give
    /// up and hand the ORIGINAL tmux error back to the caller.
    ///
    /// Without a constructible native lane the retry is guaranteed to fail, so
    /// running it would replace the one useful diagnostic (why tmux failed)
    /// with a generic "no terminal emulator found", AND leave behind a marker
    /// that permanently disables tmux for a host on which tmux is the only
    /// thing that could ever have worked.
    fn record_tmux_spawn_failure(&self, err: anyhow::Error) -> Result<()> {
        if let Err(native_err) = self.native_lane() {
            eprintln!(
                "Warning: tmux backend spawn failed and there is no native lane to retry on ({native_err}); keeping tmux selected and reporting the tmux failure."
            );
            return Err(err);
        }
        eprintln!(
            "Warning: tmux backend spawn failed ({err}); retrying on the native lane. tmux stays disabled until you re-select it with `loom run --backend tmux` — the marker survives daemon restarts."
        );
        write_fallback_marker(&self.work_dir);
        Ok(())
    }

    /// Record that a configured tmux lane was found unavailable, the FIRST
    /// time a spawn discovers it, so later spawns (and later daemon restarts)
    /// skip re-probing tmux.
    ///
    /// Same precondition as [`Self::record_tmux_spawn_failure`]: with no
    /// native lane either, this spawn fails whatever we write, and the marker
    /// would only hide a tmux installed later.
    fn note_tmux_unavailable(&self) {
        if self.configured_kind != SessionBackendKind::Tmux
            || fallback_marker_exists(&self.work_dir)
        {
            return;
        }
        if self.native_lane().is_err() {
            eprintln!(
                "Warning: terminal backend \"tmux\" is configured but unavailable, and no native terminal was detected either; not recording a fallback. Install tmux, or a terminal emulator."
            );
            return;
        }
        eprintln!(
            "Warning: terminal backend \"tmux\" is configured but unavailable, so this and every later spawn use the native lane: install tmux and re-select it with `loom run --backend tmux`, or set the [terminal] backend to \"native\"."
        );
        write_fallback_marker(&self.work_dir);
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
        if self.resolve_lane() == SessionBackendKind::Tmux {
            let mut tmux_session = session.clone();
            tmux_session.backend = SessionBackendKind::Tmux;
            match spawn_tmux(&self.tmux, tmux_session) {
                Ok(spawned) => return Ok(spawned),
                Err(err) => self.record_tmux_spawn_failure(err)?,
            }
        } else {
            self.note_tmux_unavailable();
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
