//! Reading the text of a session's tmux pane.
//!
//! This is a read of text for a report (what a parked stage's pane showed) or
//! for the history file kept past the session (`super::history`), never a
//! liveness signal: the monitor derives life and death from verified process
//! identity only (see `TmuxBackend::is_session_alive`) and must never consult
//! this module for it.

use crate::models::session::Session;

use super::viewer::tmux_session_name;
use super::{run_tmux_control, socket_name, socket_path_for, TMUX_PROBE_TIMEOUT};

/// Pure builder for the `tmux capture-pane` argv (excluding the `tmux`
/// binary): joined wrapped lines (`-J`), the last `lines` rows of scrollback
/// plus the visible screen (`-S -<lines>`). `target` is passed to `-t` as it
/// is (see [`exact_pane_target`]). Arguments are passed as argv elements, so
/// nothing is shell-quoted.
pub(super) fn capture_pane_argv(socket: &str, target: &str, lines: usize) -> Vec<String> {
    ["-L", socket, "capture-pane", "-p", "-J", "-t", target, "-S"]
        .iter()
        .map(|part| (*part).to_string())
        .chain(std::iter::once(format!("-{lines}")))
        .collect()
}

/// Pure builder for the argv that reads a pane's whole scrollback with its
/// colors: `-e` keeps the SGR sequences, `-J` joins wrapped lines, and
/// `-S -` / `-E -` run from the start of the history to the end of the screen.
pub(super) fn capture_history_argv(socket: &str, target: &str) -> Vec<String> {
    [
        "-L",
        socket,
        "capture-pane",
        "-p",
        "-e",
        "-J",
        "-t",
        target,
        "-S",
        "-",
        "-E",
        "-",
    ]
    .iter()
    .map(|part| (*part).to_string())
    .collect()
}

/// The `capture-pane` target for the active pane of the session named exactly
/// `session_name`. tmux resolves a bare name by exact match, then by prefix, then
/// by glob, so a session that is gone could be answered by another whose name
/// starts the same way. A leading `=` allows an exact match only, and the
/// trailing `:` makes it a window target (the session's current window, so its
/// active pane) instead of a pane or window name.
fn exact_pane_target(session_name: &str) -> String {
    format!("={session_name}:")
}

/// What `argv` (built from the socket and pane target) prints for `session`'s
/// own tmux pane, or `None` when the session has no tmux name, its server
/// socket does not exist (a dead or never-created server costs no subprocess),
/// or the capture fails.
///
/// Bounded by `TMUX_PROBE_TIMEOUT` because the orchestrator's single poll
/// thread calls it.
fn capture_stdout(
    session: &Session,
    argv: impl FnOnce(&str, &str) -> Vec<String>,
) -> Option<Vec<u8>> {
    let session_name = tmux_session_name(session)?;
    let socket = socket_name(session);
    if !socket_path_for(&socket).exists() {
        return None;
    }
    let args = argv(&socket, &exact_pane_target(&session_name));
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = run_tmux_control(
        &args,
        TMUX_PROBE_TIMEOUT,
        format!("tmux capture-pane ({socket})"),
    )
    .ok()?;
    output.status.success().then_some(output.stdout)
}

/// The raw text of the last `lines` rows of `session`'s own tmux pane, or
/// `None` when it cannot be read. The text is returned as captured; callers
/// normalise it.
pub(crate) fn capture_pane_tail(session: &Session, lines: usize) -> Option<String> {
    capture_stdout(session, |socket, target| {
        capture_pane_argv(socket, target, lines)
    })
    .map(|stdout| String::from_utf8_lossy(&stdout).into_owned())
}

/// `session`'s whole scrollback and screen, colors included, or `None` when
/// the pane cannot be read.
pub(super) fn capture_history(session: &Session) -> Option<Vec<u8>> {
    capture_stdout(session, capture_history_argv)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pane_target_names_its_session_exactly() {
        assert_eq!(exact_pane_target("loom-stage"), "=loom-stage:");
    }

    #[test]
    fn the_history_argv_reads_the_whole_scrollback_with_colors() {
        let argv = capture_history_argv("loom-socket", &exact_pane_target("loom-stage"));
        assert_eq!(
            argv,
            [
                "-L",
                "loom-socket",
                "capture-pane",
                "-p",
                "-e",
                "-J",
                "-t",
                "=loom-stage:",
                "-S",
                "-",
                "-E",
                "-"
            ]
        );
    }

    #[test]
    fn the_capture_argv_carries_the_exact_target() {
        let argv = capture_pane_argv("loom-socket", &exact_pane_target("loom-stage"), 40);
        let target = argv.iter().position(|arg| arg == "-t").unwrap() + 1;
        assert_eq!(argv[target], "=loom-stage:");
    }
}
