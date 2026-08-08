//! `loom attach [stage-id]` — attach to tmux-hosted loom sessions.
//!
//! Direct (`stage_id` given): `exec`s straight into one stage's own tmux
//! server, replacing the loom process with the attach client. Overview
//! (`stage_id` omitted): builds a separate per-repo "viewer" tmux server
//! whose panes each host a nested attach client into one live session's own
//! server, tiled side by side, then `exec`s into the viewer.
//!
//! Native-backend sessions are out of scope: they already own a visible OS
//! terminal window, so there is nothing for this command to attach to.

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use std::io::IsTerminal;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;

use crate::commands::common::find_work_dir;
use crate::models::session::{Session, SessionBackendKind, SessionStatus};
use crate::orchestrator::terminal::native::NativeBackend;
use crate::orchestrator::terminal::tmux::{socket_name, TmuxBackend};
use crate::parser::frontmatter::parse_from_markdown;

/// tmux session name of the tiled viewer. Fixed; the SOCKET is what varies per repo.
const OVERVIEW_SESSION: &str = "loom-overview";

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
/// Emitted right after `new-session`, NOT after the splits: `remain-on-exit`
/// is a WINDOW option, so it governs every pane in the window including ones
/// created later — setting it early also protects a pane that dies DURING
/// the build itself (the realistic race is a loom session ending between the
/// liveness scan and its own split). Pane 0 is still briefly unprotected
/// between `new-session` and this step; if it dies there, the follow-up
/// `has-session` probe in [`run_overview_step`] reports the failure instead
/// of the command proceeding blindly.
const REMAIN_ON_EXIT_FLAGS: &[&str] = &[
    "set-option",
    "-w",
    "-t",
    OVERVIEW_SESSION,
    "remain-on-exit",
    "on",
];

/// Entry point. `stage_id == None` => tiled overview of every live tmux
/// session; `Some(id)` => attach straight into that stage's session.
pub fn execute(stage_id: Option<String>) -> Result<()> {
    let work_dir = find_work_dir()?;
    let sessions = live_tmux_sessions(&work_dir)?;

    if sessions.is_empty() {
        return report_no_live_sessions(&work_dir);
    }

    match stage_id {
        Some(id) => attach_direct(&sessions, &id),
        None => run_overview(&work_dir, &sessions),
    }
}

/// Every live tmux-hosted session recorded in `<work_dir>/sessions`, oldest first.
fn live_tmux_sessions(work_dir: &Path) -> Result<Vec<Session>> {
    let Ok(entries) = std::fs::read_dir(work_dir.join("sessions")) else {
        return Ok(Vec::new());
    };

    // Constructed ONCE, outside the loop: `TmuxBackend::new` is infallible
    // (unlike `NativeBackend::new`, it never probes for a terminal), so there
    // is no reason to pay for repeated construction per session.
    let backend = TmuxBackend::new(work_dir.to_path_buf());

    let mut sessions = Vec::new();
    for entry in entries.flatten() {
        // Same rule as commands/sessions.rs::list(): only files with an
        // explicit `.md` extension count. Spelled `is_none_or` rather than
        // `!…is_some_and` because clippy::nonminimal_bool rejects the latter.
        if entry.path().extension().is_none_or(|ext| ext != "md") {
            continue;
        }
        if let Some(session) = load_live_tmux_session(&backend, &entry.path())? {
            sessions.push(session);
        }
    }

    // Deterministic pane order / "newest wins", not filesystem-order dependent.
    sessions.sort_by(|a, b| (a.created_at, &a.id).cmp(&(b.created_at, &b.id)));

    Ok(sessions)
}

/// Parse one session file, returning it only if live and tmux-hosted.
/// `Ok(None)` covers everything filtered out, including an unreadable or
/// corrupt file (e.g. read mid-write by the daemon) — never fail the whole
/// command over one bad session file.
fn load_live_tmux_session(backend: &TmuxBackend, path: &Path) -> Result<Option<Session>> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Ok(None);
    };
    let Ok(session) = parse_from_markdown::<Session>(&content, "Session") else {
        return Ok(None);
    };

    if session.backend != SessionBackendKind::Tmux {
        return Ok(None);
    }
    if !matches!(
        session.status,
        SessionStatus::Running | SessionStatus::Spawning
    ) {
        return Ok(None);
    }
    if !backend.is_session_alive(&session)? {
        return Ok(None);
    }
    if tmux_session_name(&session).is_none() {
        // Nothing to attach to (no tracking_key and no stage_id).
        return Ok(None);
    }

    Ok(Some(session))
}

/// The tmux session name a spawned session was created under.
///
/// NOT sourced from `TmuxBackend::spawn` calling `window_title_and_pid_key`
/// — it doesn't: `prepare_session_launch` (`native/launch.rs:50`) sets
/// `title = session.tracking_key.clone()` directly, and that title is what
/// `spawn_in_tmux` uses as the tmux session name. This function agrees with
/// it only because `window_title_and_pid_key` itself returns `tracking_key`
/// whenever it is non-empty; the `loom-<stage_id>` fallback branch below is
/// therefore unreachable for any session that has actually been spawned
/// (`assign_to_stage` always sets `tracking_key` first).
fn tmux_session_name(session: &Session) -> Option<String> {
    NativeBackend::window_title_and_pid_key(session).map(|(title, _)| title)
}

/// Per-REPOSITORY viewer socket name, `loom-view-<8 hex>`. The tmux socket
/// directory is per-USER, not per-repo, so a fixed global name would make two
/// checkouts collide — and the overview's own best-effort `kill-session`
/// would then tear down the other repo's viewer.
fn viewer_socket_name(work_dir: &Path) -> String {
    // `.work` is a SYMLINK to the main repo's `.work` in a worktree, so
    // canonicalizing gives the same path from every worktree of the repo.
    let canonical = work_dir
        .canonicalize()
        .unwrap_or_else(|_| work_dir.to_path_buf());
    let repo_root = canonical.parent().unwrap_or(&canonical);

    let mut hasher = Sha256::new();
    hasher.update(repo_root.as_os_str().as_bytes());
    let digest = hasher.finalize();
    let hex = hex::encode(digest);

    // `/tmp/tmux-<uid>/loom-view-xxxxxxxx` is ~31 bytes, far inside the
    // 104-byte AF_UNIX `sun_path` limit.
    format!("loom-view-{}", &hex[..8])
}

/// Identity for loom's alphanumeric/dash ids; defence in depth because the
/// result is handed to `/bin/sh -c`.
fn escape_arg(s: &str) -> String {
    shell_escape::escape(Cow::Borrowed(s)).into_owned()
}

/// A nested attach client into one session's own tmux server. `unset TMUX`
/// first: the pane inherits `$TMUX` from the viewer server and tmux refuses a
/// nested attach otherwise. `exec` so pane death stays meaningful under
/// remain-on-exit.
///
/// This string is always the `-c` payload of an EXPLICIT `sh -c` (see
/// [`build_overview_argv`]), never handed to tmux's own `default-shell`.
/// `unset` is not portable: under `default-shell=/bin/csh` (verified on tmux
/// 3.7b) `unset` clears a shell variable, never the environment, so the
/// nested attach still sees `$TMUX` and refuses it; fish has no `unset` at
/// all. A guaranteed POSIX `sh` sidesteps the operator's login shell
/// entirely, and makes `escape_arg`'s `sh`-flavoured quoting provably
/// correct rather than accidentally so.
fn pane_command(session_socket: &str, tmux_session: &str) -> String {
    format!(
        "unset TMUX; exec tmux -L {} attach-session -t {}",
        escape_arg(session_socket),
        escape_arg(tmux_session)
    )
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

    // Every step shares the `-L <viewer_socket>` prefix; `pane`, when given,
    // appends `"sh", "-c", <pane command>` as three separate argv words so
    // tmux runs it under a guaranteed shell instead of `default-shell`.
    let step = |rest: &[&str], pane: Option<&(String, String)>| -> Vec<String> {
        let mut argv = vec!["-L".to_string(), viewer_socket.to_string()];
        argv.extend(rest.iter().map(|s| s.to_string()));
        if let Some((socket, tmux_session)) = pane {
            argv.push("sh".to_string());
            argv.push("-c".to_string());
            argv.push(pane_command(socket, tmux_session));
        }
        argv
    };

    let mut steps = vec![
        // (a) Best-effort teardown of this repo's previous overview.
        step(&["kill-session", "-t", OVERVIEW_SESSION], None),
        // (b) Create the viewer, first pane already running an attach client.
        step(NEW_SESSION_FLAGS, Some(&panes[0])),
        // (c) Set EARLY (window option, see REMAIN_ON_EXIT_FLAGS docs):
        // protects every pane created below, including one that dies
        // mid-build.
        step(REMAIN_ON_EXIT_FLAGS, None),
    ];

    // (d) One more pane per remaining session, RE-TILED AFTER EVERY SPLIT.
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
fn run_overview(work_dir: &Path, sessions: &[Session]) -> Result<()> {
    // Do not build a viewer we could not then attach to.
    require_tty()?;

    let viewer_socket = viewer_socket_name(work_dir);

    // Discovery already filtered out sessions with no resolvable tmux
    // session name, so `unwrap_or_default` here never actually fires.
    let panes: Vec<(String, String)> = sessions
        .iter()
        .map(|s| (socket_name(s), tmux_session_name(s).unwrap_or_default()))
        .collect();

    let steps = build_overview_argv(&viewer_socket, &panes);
    for argv in &steps {
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
    let verb = argv.get(2).map(String::as_str);

    let output = Command::new("tmux")
        .args(argv)
        .output()
        .with_context(|| format!("Failed to run tmux {}", argv.join(" ")))?;

    match verb {
        // Best-effort: there may be no previous overview to tear down.
        Some("kill-session") => Ok(()),
        Some("new-session") => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // `new-session` exits 0 EVEN WHEN THE SERVER COULD NOT BE
            // CREATED (it prints to stderr). We must never `exec` onto an
            // unverified server, because `exec` replaces the loom process
            // and makes any later failure unreportable. This exact
            // exit-0-with-stderr rule is shared with the per-session spawn
            // lane and pinned by tests there — reuse it rather than
            // re-implementing an untested copy — then verify with a
            // follow-up `has-session` before trusting this step.
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
        _ => {
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                bail!("tmux {} failed: {stderr}", argv.join(" "));
            }
            Ok(())
        }
    }
}

/// Every live session assigned to `stage_id`. Split out of `attach_direct`
/// purely for testability — together with [`pick_newest`] it IS the selection
/// invariant this command exists to get right, and unlike the rest of
/// `attach_direct` the pair needs neither tmux, a TTY, nor `exec` to exercise.
fn matches_for_stage<'a>(sessions: &'a [Session], stage_id: &str) -> Vec<&'a Session> {
    sessions
        .iter()
        .filter(|s| s.stage_id.as_deref() == Some(stage_id))
        .collect()
}

/// Pick the most-recently-created session from an already-filtered slice
/// (`max_by_key` returns the LAST maximum, the determinism this command
/// relies on). Kept separate from [`matches_for_stage`] so `attach_direct`
/// can report `matches.len()` without filtering the same predicate twice.
fn pick_newest<'a>(candidates: &[&'a Session]) -> Option<&'a Session> {
    candidates.iter().copied().max_by_key(|s| s.created_at)
}

/// Attach straight into the session hosting `stage_id`.
fn attach_direct(sessions: &[Session], stage_id: &str) -> Result<()> {
    let matches = matches_for_stage(sessions, stage_id);

    let Some(target) = pick_newest(&matches) else {
        let mut live_ids: Vec<&str> = sessions
            .iter()
            .filter_map(|s| s.stage_id.as_deref())
            .collect();
        live_ids.sort_unstable();
        live_ids.dedup();
        let ids_display = if live_ids.is_empty() {
            "(none)".to_string()
        } else {
            live_ids.join(", ")
        };
        bail!("No live tmux session for stage '{stage_id}'. Live stage ids: {ids_display}");
    };

    if matches.len() > 1 {
        println!(
            "Found {} live sessions for stage '{stage_id}'; attaching to the newest (session {})",
            matches.len(),
            target.id
        );
    }

    // Not `find_session_for_stage`: it returns the FIRST session file in
    // filesystem order without checking liveness. The live set above is correct.
    require_tty()?;

    // Discovery already guaranteed `Some` for every session here.
    let tmux_session = tmux_session_name(target).unwrap_or_default();
    exec_tmux(&[
        "-L",
        &socket_name(target),
        "attach-session",
        "-t",
        &tmux_session,
    ])
}

/// `exec` into tmux, replacing the loom process. Only ever returns an error:
/// `CommandExt::exec` returns solely on failure.
fn exec_tmux(argv: &[&str]) -> Result<()> {
    // `.env_remove("TMUX")` so attaching works even when `loom attach` itself
    // is run from inside another tmux session.
    let err = Command::new("tmux").args(argv).env_remove("TMUX").exec();
    Err(anyhow::Error::new(err).context(format!("Failed to exec tmux {}", argv.join(" "))))
}

/// Refuse to exec tmux when stdout is not a terminal — tmux would otherwise
/// fail obscurely ("open terminal failed").
fn require_tty() -> Result<()> {
    if !std::io::stdout().is_terminal() {
        bail!("loom attach must be run from a terminal");
    }
    Ok(())
}

/// Explain an empty live set, choosing the message from the CONFIGURED
/// backend. Consulted ONLY here, to pick the wording — gating the whole
/// command on it would be wrong, since live tmux-hosted sessions from before
/// a config flip to native must stay attachable.
fn report_no_live_sessions(work_dir: &Path) -> Result<()> {
    let config = crate::fs::work_dir::read_terminal_config(work_dir)?;
    match config.backend {
        SessionBackendKind::Native => bail!(
            "loom attach requires the tmux backend (set [terminal] backend = \"tmux\" in .work/config.toml or run loom run --backend tmux)"
        ),
        SessionBackendKind::Tmux => {
            println!("No live tmux sessions");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests;
