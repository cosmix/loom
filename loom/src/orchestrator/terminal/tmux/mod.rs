//! Tmux terminal backend
//!
//! Spawns sessions in a headless tmux server, one per session, so loom can
//! run without a GUI terminal emulator. Each session gets its own tmux
//! socket (see [`socket_name`]) rather than sharing one server, so killing
//! one session can never take down another.
//!
//! Session-launch preparation (prompt, model/effort policy, permission mode,
//! wrapper script) is shared with the native lane via
//! `super::native::prepare_session_launch` — this module only owns the
//! tmux-specific spawn/kill/liveness mechanics.
//!
//! Liveness is PID-based ONLY, never `tmux has-session` — see
//! [`TmuxBackend::is_session_alive`] for why a tmux-server-alive check would
//! silently defeat crash detection.

mod socket;

use anyhow::{Context, Result};
use shell_escape::escape;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::models::session::{Session, SessionType};
use crate::models::stage::Stage;
use crate::models::worktree::Worktree;

use super::native;

pub use socket::{
    kill_socket_server, list_loom_sockets, socket_path_for, socket_session_is_alive, LoomSocket,
};

const TMUX_SPAWN_TIMEOUT: Duration = Duration::from_secs(20);
const TMUX_PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const TMUX_TEARDOWN_TIMEOUT: Duration = Duration::from_secs(5);

fn run_tmux_command(
    command: &mut Command,
    timeout: Duration,
    operation: impl Into<String>,
) -> Result<std::process::Output> {
    crate::process::apply_stage_environment(command);
    crate::process::run_bounded_output(command, timeout, operation)
}

pub(super) fn run_tmux_control(
    args: &[&str],
    timeout: Duration,
    operation: impl Into<String>,
) -> Result<std::process::Output> {
    let mut command = Command::new("tmux");
    command.args(args);
    run_tmux_command(&mut command, timeout, operation)
}

/// Per-session tmux socket name.
///
/// Keyed on `session.id` (~25 chars, `session-<uuid8>-<unixts>`), NOT on the
/// stage id: plan stage ids run up to 128 chars and would risk exceeding the
/// 104-byte `AF_UNIX sun_path` limit once joined with the socket directory.
/// Windows/panes are still named with `session.tracking_key` for human
/// listing (see `TmuxBackend::spawn`).
pub fn socket_name(session: &Session) -> String {
    format!("loom-{}", session.id)
}

/// Pure builder for the `tmux new-session` argv (excluding the `tmux` binary
/// itself), so the exact argument list is unit-testable without running
/// tmux.
///
/// `command` is shell-escaped here, EXACTLY ONCE: tmux interprets its
/// trailing argument as a shell command line, so an unescaped wrapper path
/// containing a space, quote, or shell metacharacter would be misparsed or
/// worse. Callers must pass the raw path, never a pre-escaped string.
fn new_session_argv(socket: &str, session_name: &str, cwd: &Path, command: &Path) -> Vec<String> {
    let escaped_command = escape(command.to_string_lossy()).into_owned();
    vec![
        "-L".to_string(),
        socket.to_string(),
        "new-session".to_string(),
        "-d".to_string(),
        "-s".to_string(),
        session_name.to_string(),
        "-x".to_string(),
        "220".to_string(),
        "-y".to_string(),
        "50".to_string(),
        "-c".to_string(),
        cwd.to_string_lossy().to_string(),
        escaped_command,
    ]
}

/// Decide whether a `tmux new-session` invocation actually succeeded.
///
/// Split out as a pure function precisely so the exit-0-with-stderr case can
/// be pinned by a unit test. That case is NOT portably reproducible as a real
/// tmux invocation: an unwritable socket parent makes tmux exit **1**
/// (`couldn't create directory … (Permission denied)`), whereas the silent
/// failure this guards against needs the directory to exist while socket
/// creation itself is denied — a sandbox/seccomp condition no CI runner can be
/// relied on to reproduce. Testing the decision directly is what keeps the
/// rule honest.
pub(crate) fn evaluate_new_session(socket: &str, status_success: bool, stderr: &str) -> Result<()> {
    if !status_success || !stderr.trim().is_empty() {
        anyhow::bail!("tmux new-session failed for socket '{socket}': {stderr}");
    }
    Ok(())
}

/// Start a tmux session on `socket` running `command` (typically a loom
/// wrapper script) in `cwd`.
///
/// # Success detection
///
/// Verified on tmux 3.7b: when the server cannot create its socket, tmux
/// prints `error creating <path> (Operation not permitted)` to stderr and
/// STILL EXITS 0. An exit-code-only check would therefore report a total
/// failure as success. This treats the spawn as successful only when ALL of:
/// 1. `new-session`'s exit status is 0, AND
/// 2. its stderr is empty, AND
/// 3. a follow-up `has-session` probe against the same socket exits 0.
///
/// `has-session` here is a SPAWN-TIME success probe only — never a liveness
/// source (see [`TmuxBackend::is_session_alive`]).
pub fn spawn_in_tmux(socket: &str, session_name: &str, cwd: &Path, command: &Path) -> Result<()> {
    let argv = new_session_argv(socket, session_name, cwd, command);
    let mut command = Command::new("tmux");
    command.args(&argv);
    let output = run_tmux_command(
        &mut command,
        TMUX_SPAWN_TIMEOUT,
        format!("tmux new-session ({socket})"),
    )
    .with_context(|| format!("Failed to spawn tmux new-session on socket '{socket}'"))?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    evaluate_new_session(socket, output.status.success(), &stderr)?;

    let probe = run_tmux_control(
        &["-L", socket, "has-session", "-t", session_name],
        TMUX_PROBE_TIMEOUT,
        format!("tmux has-session ({socket})"),
    )
    .with_context(|| {
        format!("Failed to probe tmux session '{session_name}' on socket '{socket}'")
    })?;
    if !probe.status.success() {
        let probe_stderr = String::from_utf8_lossy(&probe.stderr);
        anyhow::bail!(
            "tmux has-session failed for '{session_name}' on socket '{socket}': {probe_stderr}"
        );
    }

    // Best-effort: hide the status bar. Cosmetic only — never fails the spawn.
    let _ = run_tmux_control(
        &["-L", socket, "set-option", "-g", "status", "off"],
        TMUX_PROBE_TIMEOUT,
        format!("tmux set-option ({socket})"),
    );

    Ok(())
}

/// Wait for the wrapper script's `exec`'d process to become discoverable on
/// `pid_key`, the same layered wait the native lane uses.
///
/// `fallback_pid` is deliberately `None`: unlike the native lane, there is no
/// terminal PID to fall back to, so a session whose wrapper PID never
/// appears must error rather than silently report a bogus PID.
pub fn await_tmux_session_pid(
    work_dir: &Path,
    pid_key: &str,
    cwd: &Path,
    session_id: &str,
) -> Result<u32> {
    native::await_session_pid(work_dir, pid_key, cwd, session_id, None)
}

/// Tear down the tmux server owned by `socket` and unlink its socket file.
///
/// Both steps are needed and neither implies the other: verified that
/// `kill-server` does not always unlink its own socket (the stale file can
/// linger with no process behind it), while removing the file without killing
/// the server would strand the server AND lose the only handle to it.
///
/// Best-effort by design — this runs on teardown and error paths where there
/// is nothing useful to do with a failure.
fn teardown_socket(socket: &str) {
    let socket_path = socket_path_for(socket);
    let _ = kill_socket_server(&socket_path);
    let _ = std::fs::remove_file(&socket_path);
}

/// Undo a partially-completed tmux spawn: kill whatever server came up, then
/// delete the PID file and wrapper script this attempt created.
///
/// Split out of [`TmuxBackend::spawn`]'s error paths so the teardown itself is
/// unit-testable without a tmux server or a claude process — the ordering is
/// load-bearing (kill the process BEFORE removing the PID file it writes to,
/// or a slow wrapper can recreate the file behind us) and so is the file
/// removal (see `TmuxBackend::spawn`'s docs: a surviving PID file makes the
/// native retry adopt the tmux attempt's PID).
fn abort_tmux_spawn(work_dir: &Path, socket: &str, pid_key: &str) {
    teardown_socket(socket);
    native::cleanup_stage_files(work_dir, pid_key);
}

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
    /// [`evaluate_new_session`] rejects an exit-0 spawn whose stderr carried
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
    /// Hence one `abort` closure ([`abort_tmux_spawn`]), used by every error
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

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_spawn;
