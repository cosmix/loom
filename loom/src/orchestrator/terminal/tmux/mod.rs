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
//!
//! Two submodules serve `loom attach`'s tiled overview rather than the spawn
//! lane: `viewer` owns the viewer's identity and the "which sessions are
//! attachable" filter, and `reconcile` uses them to keep an already-built
//! viewer in sync with session reality on every scheduler tick. They live here,
//! not under `commands/attach`, because the daemon must not depend on a command
//! module — and because one shared definition is what stops the one-shot build
//! and the live reconciler from disagreeing about who is attachable.

mod reconcile;
mod socket;
/// `pub` (not `pub(crate)`) rather than re-exported piecemeal: `commands/attach`
/// consumes most of this module, its tests share `viewer::tests::stub_session`,
/// and the real-tmux e2e test (`tests/e2e/tmux_reconcile.rs`) needs
/// `viewer_socket_name`, `pane_command`, and `OVERVIEW_SESSION` to mirror
/// `loom attach`'s own build sequence from outside the crate.
pub mod viewer;

use anyhow::{Context, Result};
use shell_escape::escape;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use crate::models::session::Session;

use super::native;

pub use socket::{
    kill_socket_server, list_loom_sockets, socket_path_for, socket_session_is_alive, LoomSocket,
};

/// The daemon's one entry point into overview maintenance. Re-exported here so
/// the scheduler loop names the tmux backend rather than reaching into a
/// submodule for what is, from its side, a single best-effort call.
pub(crate) use reconcile::refresh_attached_viewer;

/// `pub`, not `pub(crate)`: lets the real-tmux e2e test
/// (`tests/e2e/tmux_reconcile.rs`) drive the reconciler directly against a
/// tmux server it stands up itself, rather than only through the daemon.
pub use reconcile::reconcile_viewer;

/// Server options loom forces on every stage server it creates, applied
/// best-effort after the spawn.
///
/// `status off` is cosmetic. The other two are not:
///
/// - `mouse off` — tmux reads the operator's `~/.tmux.conf` at
///   `start-server`, and `set -g mouse on` is a common setting. With capture
///   on, tmux's own root-table mouse bindings are armed inside agent panes
///   (including the right-click menu whose `Kill` entry ends the pane).
///
/// - `terminal-overrides[99] *:kmous@` — `mouse off` alone is NOT enough.
///   claude enables all-motion mouse tracking (1003+1006) in its pane, and
///   tmux mirrors the active pane's mouse mode out to the attached client's
///   terminal — mouse option regardless — whenever that terminal has the
///   `kmous` capability (verified in tmux 3.6a `tty.c`; and with `mouse off`,
///   incoming client mouse input is forwarded straight into the pane app,
///   bypassing key tables entirely). So the operator's drag is consumed as
///   app mouse events instead of selecting text, claude treats it as a TUI
///   selection and copies it by running `tmux load-buffer -w -` against this
///   server — and tmux 3.6a CRASHES serving `load-buffer -w` with an attached
///   client, killing the server, SIGHUP-ing claude, and presenting as
///   `server exited unexpectedly` plus a filed stage crash. Deleting `kmous`
///   for every client TERM (`*`) means no loom server ever puts a terminal
///   into mouse mode: drags stay in the emulator as native selection and no
///   event ever reaches the agent. Index 99 makes re-application idempotent
///   and leaves the operator's own override entries (e.g. truecolor) intact.
///   See `doc/loom/knowledge/mistakes/tmux-backend.md`.
///
/// The viewer applies the same overrides — see `commands/attach/overview.rs`.
const PRESENTATION_OPTIONS: &[(&str, &str)] = &[
    ("status", "off"),
    ("mouse", "off"),
    ("terminal-overrides[99]", "*:kmous@"),
];

const TMUX_SPAWN_TIMEOUT: Duration = Duration::from_secs(20);
const TMUX_PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const TMUX_TEARDOWN_TIMEOUT: Duration = Duration::from_secs(5);

/// TERM pinned on every pure control query run through [`run_tmux_control`]
/// (`has-session`, `list-panes`, `set-option`, `kill-session`, …). These
/// commands need no terminal capabilities at all, but inheriting whatever
/// TERM the operator's terminal forwarded gives them a way to fail that has
/// nothing to do with tmux server state — and that failure is
/// indistinguishable, to `endpoint_ready`, from "the server is not accepting
/// clients", so a terminfo problem presents as a dead tmux server. `dumb` is
/// guaranteed present in every terminfo database, so pinning it removes the
/// failure mode for every terminal emulator, including ones whose terminfo we
/// do not know how to locate. Forwarding TERMINFO/TERMINFO_DIRS
/// (`process::environment`) stays: it is what the paths that genuinely need a
/// real terminal — the agent wrapper, confined acceptance criteria — depend
/// on. Applied in `tmux_control_command` AFTER `apply_stage_environment`, so
/// the pin wins over whatever TERM the host forwarded.
///
/// Deliberately NOT passed to `spawn_in_tmux`'s `new-session` call: that
/// command creates the session the AGENT runs in, a different contract the
/// wrapper's own `env -i` governs.
const CONTROL_TERM_OVERRIDE: &[(&str, &str)] = &[("TERM", "dumb")];

fn run_tmux_command(
    command: &mut Command,
    timeout: Duration,
    operation: impl Into<String>,
    env_overrides: &[(&str, &str)],
) -> Result<std::process::Output> {
    crate::process::apply_stage_environment(command);
    for (key, value) in env_overrides {
        command.env(key, value);
    }
    crate::process::run_bounded_output(command, timeout, operation)
}

/// Build the fully-configured `tmux` control-query command, without running
/// it. Split out of `run_tmux_control` so its configuration — in particular,
/// that `CONTROL_TERM_OVERRIDE` is applied AFTER `apply_stage_environment`,
/// so the pin wins by construction rather than by luck of the host's ambient
/// `TERM` — is assertable on the `Command` value directly, with no
/// subprocess run and no process-global environment mutation. See
/// `tests::control_command_pins_term_dumb`.
fn tmux_control_command(args: &[&str]) -> Command {
    let mut command = Command::new("tmux");
    command.args(args);
    crate::process::apply_stage_environment(&mut command);
    for (key, value) in CONTROL_TERM_OVERRIDE {
        command.env(key, value);
    }
    command
}

pub(super) fn run_tmux_control(
    args: &[&str],
    timeout: Duration,
    operation: impl Into<String>,
) -> Result<std::process::Output> {
    let mut command = tmux_control_command(args);
    crate::process::run_bounded_output(&mut command, timeout, operation)
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
        &[],
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

    // Best-effort presentation. Never fails the spawn.
    for (option, value) in PRESENTATION_OPTIONS {
        let _ = run_tmux_control(
            &["-L", socket, "set-option", "-g", option, value],
            TMUX_PROBE_TIMEOUT,
            format!("tmux set-option {option} ({socket})"),
        );
    }

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
/// A failed server kill is uncertainty: retain the socket (the only remaining
/// control handle) and propagate the error to the fail-closed handoff path.
fn teardown_socket(socket: &str) -> Result<()> {
    let socket_path = socket_path_for(socket);
    if !socket_path.exists() {
        return Ok(());
    }
    if !kill_socket_server(&socket_path) {
        anyhow::bail!("tmux kill-server failed for {}", socket_path.display());
    }
    match std::fs::remove_file(&socket_path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error)
            .with_context(|| format!("removing killed tmux socket {}", socket_path.display())),
    }
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
    let socket_path = socket_path_for(socket);
    if socket_path.exists() && !kill_socket_server(&socket_path) {
        eprintln!(
            "Warning: failed to abort tmux server at {}; retaining its socket and PID evidence",
            socket_path.display()
        );
        return;
    }
    let _ = std::fs::remove_file(&socket_path);
    native::cleanup_stage_files(work_dir, pid_key);
}

mod backend;
pub use backend::TmuxBackend;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_spawn;
