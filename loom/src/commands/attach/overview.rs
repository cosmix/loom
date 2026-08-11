//! The tiled overview viewer — a per-repo tmux server whose panes each host a
//! nested attach client into one live session's own server.
//!
//! Split from the parent module because the viewer is a second tmux server
//! with its own lifecycle rules, distinct from the discovery and direct-attach
//! logic that surrounds it: it must be created, hardened against its own panes
//! dying, populated, and only then attached to.
//!
//! The viewer's identity (socket name, session name, pane command) and the
//! attachability filter now live in `orchestrator::terminal::tmux::viewer`,
//! shared with the daemon-side reconciler that keeps an attached overview in
//! sync; this module owns only the one-shot build sequence.

use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::{Command, Output};

use super::{exec_tmux, require_tty};
use crate::models::session::Session;
use crate::orchestrator::terminal::tmux::viewer::{
    attachable_panes, endpoint_ready, pane_command, viewer_socket_name, OVERVIEW_SESSION,
};

/// Viewer creation flags: detached, with a fixed initial geometry the real
/// client resizes on attach. Hoisted out of [`build_overview_argv`] only so
/// rustfmt's vertical expansion does not bloat that function.
const NEW_SESSION_FLAGS: &[&str] = &[
    "new-session",
    "-d",
    "-s",
    OVERVIEW_SESSION,
    "-x",
    "220",
    "-y",
    "50",
];

/// Window option that keeps a pane visible after its attach client exits,
/// so a dead inner server shows a dead-pane message instead of collapsing
/// the tiled layout.
///
/// Emitted right after `new-session` as a belt-and-braces re-assertion:
/// [`VIEWER_HARDENING`] already set the same option globally before the
/// window existed, which is what protects pane 0. This targeted copy is what
/// still applies on a tmux that rejected part of that sequence, since the
/// sequence is deliberately tolerated on failure.
const REMAIN_ON_EXIT_FLAGS: &[&str] = &[
    "set-option",
    "-w",
    "-t",
    OVERVIEW_SESSION,
    "remain-on-exit",
    "on",
];

/// Bring the viewer server up and make it survivable BEFORE any pane exists.
///
/// # Why pane 0 needs this
///
/// `new-session` creates pane 0 with an attach client into ANOTHER server
/// already running in it (see [`build_overview_argv`]). If that inner server
/// is gone — the session just ended, or has not finished spawning — the client
/// exits immediately. Under tmux's defaults the dead pane takes its window,
/// the window takes the session, and the empty server then exits too, so
/// `new-session` itself reports `server exited unexpectedly` and the whole
/// attach fails. [`REMAIN_ON_EXIT_FLAGS`] cannot close that window: it targets
/// a window that does not exist until `new-session` returns. Setting the same
/// option GLOBALLY here does, because a global option is already in force for
/// the window `new-session` goes on to create.
///
/// # Why one `;`-separated sequence and not four invocations
///
/// Load-bearing. `start-server` brings up a server with NO sessions, and the
/// default `exit-empty on` reaps exactly that server before a second `tmux`
/// process could connect to configure it — tmux's own manual documents the
/// `tmux start \; show -g` idiom for this reason. Only a single sequence lets
/// `exit-empty off` land before the empty server reaps itself.
///
/// # Ordering within the sequence
///
/// Most important first: tmux abandons the REST of a sequence when one command
/// errors, so each entry is placed after everything it must not be able to
/// abort. The `kmous@` override sits after `mouse off` because its indexed
/// array syntax is the more likely of the two to be rejected by an old tmux,
/// and a rejection must not take `mouse off` with it. `remain-on-exit-format`
/// is last because it is purely cosmetic and the one entry whose availability
/// varies across tmux builds.
///
/// # `mouse off` and `terminal-overrides[99] *:kmous@`
///
/// Not cosmetic, and not about the viewer's own behaviour. tmux reads the
/// operator's `~/.tmux.conf` at `start-server`, so `set -g mouse on` — a
/// common setting — is in force in every server loom creates; `mouse off`
/// disarms tmux's own mouse bindings (menus, copy-mode) inside the viewer.
///
/// The `kmous@` override closes the hole `mouse off` leaves open: claude
/// enables all-motion mouse tracking in its pane, the inner server mirrors
/// that to its client (the viewer pane), and the viewer would mirror it again
/// to the operator's real terminal — which then reports every drag as app
/// mouse events instead of selecting text. Those events are forwarded inward
/// to claude (with `mouse off` tmux forwards client mouse input straight to
/// the pane app), claude copies the "selection" via `tmux load-buffer -w -`,
/// and tmux 3.6a crashes serving that — the `server exited unexpectedly`
/// stage deaths. Deleting the `kmous` capability for every client TERM stops
/// tmux from ever enabling mouse mode on the operator's terminal, so drags
/// stay native emulator selection. The indexed form (`[99]`) is idempotent —
/// this sequence re-runs on every `loom attach` against the same long-lived
/// server — and preserves the operator's own override entries. The same
/// override is applied to stage servers in `orchestrator/terminal/tmux/mod.rs`,
/// which breaks the chain at its source; this one is defence in depth for the
/// operator-facing terminal.
///
/// # Cost
///
/// `exit-empty off` means this repo's viewer server outlives its last pane,
/// leaving one idle tmux process per repo (reused by the next `loom attach`,
/// not one per invocation). Accepted deliberately: the server exists to be
/// configurable before pane 0 is born, which is impossible while it reaps
/// itself. The viewer socket already has no reaper — see the note in
/// `orchestrator/terminal/tmux/socket.rs`.
const VIEWER_HARDENING: &[&str] = &[
    "start-server",
    ";",
    "set-option",
    "-g",
    "exit-empty",
    "off",
    ";",
    "set-option",
    "-g",
    "-w",
    "remain-on-exit",
    "on",
    ";",
    "set-option",
    "-g",
    "mouse",
    "off",
    ";",
    "set-option",
    "-g",
    "terminal-overrides[99]",
    "*:kmous@",
    ";",
    "set-option",
    "-g",
    "remain-on-exit-format",
    "loom: session ended (exit #{pane_dead_status}) - new sessions appear automatically while loom runs",
];

/// One step's argv, sharing the `-L <viewer_socket>` prefix every invocation
/// needs. `pane`, when given, appends `"sh", "-c", <pane command>` as three
/// separate argv words so tmux runs it under a guaranteed shell instead of
/// `default-shell`.
fn overview_step(
    viewer_socket: &str,
    rest: &[&str],
    pane: Option<&(String, String)>,
) -> Vec<String> {
    let mut argv = vec!["-L".to_string(), viewer_socket.to_string()];
    argv.extend(rest.iter().map(|s| s.to_string()));
    if let Some((socket, tmux_session)) = pane {
        argv.push("sh".to_string());
        argv.push("-c".to_string());
        argv.push(pane_command(socket, tmux_session));
    }
    argv
}

/// Pure builder for the overview's tmux invocations, in order (argv AFTER the
/// `tmux` binary), kept free of tmux/filesystem/session-model so the exact
/// sequence is unit-testable. `panes` is `(session_socket, tmux_session)`.
/// Empty `panes` returns an empty vec; the caller guarantees non-empty.
///
/// Every pane command runs under an EXPLICIT `sh -c` (three separate argv
/// words), never tmux's own `default-shell`: that option is the OPERATOR's
/// login shell, not a guaranteed POSIX `/bin/sh` — see [`pane_command`]'s
/// doc comment for the csh/fish evidence.
fn build_overview_argv(viewer_socket: &str, panes: &[(String, String)]) -> Vec<Vec<String>> {
    if panes.is_empty() {
        return Vec::new();
    }
    let step = |rest: &[&str], pane: Option<&(String, String)>| -> Vec<String> {
        overview_step(viewer_socket, rest, pane)
    };

    let mut steps = vec![
        // (a) Best-effort teardown of this repo's previous overview.
        step(&["kill-session", "-t", OVERVIEW_SESSION], None),
        // (b) Bring the server up and make a dying pane 0 survivable, BEFORE
        // pane 0 exists. Must follow (a): killing the last session of a
        // server still running the defaults takes the server with it.
        step(VIEWER_HARDENING, None),
        // (c) Create the viewer, first pane already running an attach client.
        step(NEW_SESSION_FLAGS, Some(&panes[0])),
        // (d) Re-assert on the window itself — see REMAIN_ON_EXIT_FLAGS docs.
        step(REMAIN_ON_EXIT_FLAGS, None),
    ];

    // (e) One more pane per remaining session, RE-TILED AFTER EVERY SPLIT.
    // `split-window -t <session>` always targets the session's CURRENT pane
    // — the one the previous split just created — so heights halve on each
    // split. Verified on tmux 3.7b: left untiled, a detached 220x50 window
    // runs out of room on the SIXTH live session (50 -> 25 -> 12 -> 5 -> 2,
    // then the next split fails with "no space for a new pane"). Re-tiling
    // after every split keeps each pane large enough for the next split.
    for pane in &panes[1..] {
        steps.push(step(&["split-window", "-t", OVERVIEW_SESSION], Some(pane)));
        steps.push(step(
            &["select-layout", "-t", OVERVIEW_SESSION, "tiled"],
            None,
        ));
    }

    steps
}

/// Build the tiled viewer, then exec into it.
pub(super) fn run_overview(work_dir: &Path, sessions: &[Session]) -> Result<()> {
    let panes = attachable_panes(sessions, endpoint_ready);

    // Reported BEFORE the TTY check, like every other diagnostic this command
    // emits: a session that is alive but not yet attachable is the single most
    // likely reason to run `loom attach` a moment too early, and answering
    // that needs no terminal. Without this the build would proceed into a
    // `new-session` whose pane 0 dies on contact, and the operator would get
    // tmux's `server exited unexpectedly` instead of the actual reason.
    if panes.is_empty() {
        bail!(
            "{} live session(s), but none of their tmux servers are accepting clients yet — \
             they are still spawning, or have just ended. Re-run `loom attach` in a moment.",
            sessions.len()
        );
    }

    // Do not build a viewer we could not then attach to.
    require_tty()?;

    let viewer_socket = viewer_socket_name(work_dir);
    for argv in &build_overview_argv(&viewer_socket, &panes) {
        run_overview_step(&viewer_socket, argv)?;
    }

    println!("Tiling {} live session(s) in the overview", panes.len());

    exec_tmux(&[
        "-L",
        &viewer_socket,
        "attach-session",
        "-t",
        OVERVIEW_SESSION,
    ])
}

/// Execute one step of the overview build sequence, dispatching on the VERB
/// (`argv[2]`) rather than on position — keeps the executor decoupled from
/// the exact ordering `build_overview_argv` chooses.
fn run_overview_step(viewer_socket: &str, argv: &[String]) -> Result<()> {
    let output = Command::new("tmux")
        .args(argv)
        .output()
        .with_context(|| format!("Failed to run tmux {}", argv.join(" ")))?;

    match argv.get(2).map(String::as_str) {
        // Best-effort: there may be no previous overview to tear down.
        Some("kill-session") => Ok(()),
        // Best-effort: [`VIEWER_HARDENING`] only makes a dying pane
        // SURVIVABLE. A tmux that rejects part of it (no `exit-empty`, or
        // `remain-on-exit-format` filed in another option table) must not fail
        // the attach outright — the worst case is exactly the behaviour that
        // shipped before this step existed, and [`REMAIN_ON_EXIT_FLAGS`]
        // after `new-session` still applies.
        Some("start-server") => Ok(()),
        Some("new-session") => verify_viewer_session(viewer_socket, &output),
        _ => {
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                bail!("tmux {} failed: {stderr}", argv.join(" "));
            }
            Ok(())
        }
    }
}

/// Confirm `new-session` really produced a viewer we can `exec` onto.
///
/// `new-session` exits 0 EVEN WHEN THE SERVER COULD NOT BE CREATED (it prints
/// to stderr). We must never `exec` onto an unverified server, because `exec`
/// replaces the loom process and makes any later failure unreportable. The
/// exit-0-with-stderr rule is shared with the per-session spawn lane and
/// pinned by tests there — reuse it rather than re-implementing an untested
/// copy — then verify with a follow-up `has-session` before trusting the step.
fn verify_viewer_session(viewer_socket: &str, output: &Output) -> Result<()> {
    let stderr = String::from_utf8_lossy(&output.stderr);
    crate::orchestrator::terminal::tmux::evaluate_new_session(
        viewer_socket,
        output.status.success(),
        &stderr,
    )?;

    let probe = Command::new("tmux")
        .args(["-L", viewer_socket, "has-session", "-t", OVERVIEW_SESSION])
        .output()
        .with_context(|| "Failed to probe viewer overview session")?;
    if !probe.status.success() {
        let probe_stderr = String::from_utf8_lossy(&probe.stderr);
        bail!("tmux has-session failed for viewer '{viewer_socket}': {probe_stderr}");
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests/overview.rs"]
mod tests;
#[cfg(test)]
#[path = "tests/overview_hardening.rs"]
mod tests_hardening;
