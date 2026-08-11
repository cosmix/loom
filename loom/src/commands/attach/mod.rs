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
//!
//! Both paths apply one precondition the rest of loom deliberately does not:
//! the target's tmux server must be accepting clients *now*. That is a
//! strictly additional attach-time check, never a liveness source — see
//! `tmux_endpoint_ready`.

mod overview;

use anyhow::{bail, Result};
use std::io::IsTerminal;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;

use crate::commands::common::find_work_dir;
use crate::models::session::{Session, SessionBackendKind, SessionStatus};
use crate::orchestrator::terminal::native::NativeBackend;
use crate::orchestrator::terminal::tmux::{socket_name, socket_path_for, TmuxBackend};
use crate::parser::frontmatter::parse_from_markdown;
use overview::run_overview;

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

/// Whether `session`'s OWN tmux server will accept an attach client right now.
///
/// # This is not liveness and must never become liveness
///
/// Liveness is PID-only, deliberately — `architecture/terminal-backends.md`
/// ("Liveness Uses Verified Process Identity, Not tmux") and
/// [`crate::orchestrator::terminal::tmux::TmuxBackend::is_session_alive`] both
/// spell out why: a server whose pane process died but which has not reaped
/// itself still answers `has-session` with exit 0, so consulting it there
/// would report a dead agent as alive, and the crash would never be filed or
/// retried.
///
/// Attaching asks a different question, and the two answers genuinely
/// disagree in BOTH directions. A session mid-spawn has a live wrapper PID
/// before its server accepts clients; a session whose server was just torn
/// down can keep a live claude PID for a moment after. Either way a pane
/// running `attach-session` against it exits instantly — which is precisely
/// what took the whole viewer down before [`overview::VIEWER_HARDENING`]
/// existed.
///
/// So this is an ADDITIONAL precondition on the attach path only. It never
/// substitutes for `is_session_alive`, and nothing outside this module may
/// call it.
fn tmux_endpoint_ready(session: &Session, tmux_session: &str) -> bool {
    let socket = socket_name(session);
    if !socket_path_for(&socket).exists() {
        return false;
    }
    Command::new("tmux")
        .args(["-L", &socket, "has-session", "-t", tmux_session])
        .output()
        .is_ok_and(|probe| probe.status.success())
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

/// Name every stage that DOES have a live session, so an unknown or misspelled
/// stage id is answered with the choices rather than a bare refusal.
fn live_stage_ids(sessions: &[Session]) -> String {
    let mut live_ids: Vec<&str> = sessions
        .iter()
        .filter_map(|s| s.stage_id.as_deref())
        .collect();
    live_ids.sort_unstable();
    live_ids.dedup();
    if live_ids.is_empty() {
        "(none)".to_string()
    } else {
        live_ids.join(", ")
    }
}

/// Attach straight into the session hosting `stage_id`.
fn attach_direct(sessions: &[Session], stage_id: &str) -> Result<()> {
    let matches = matches_for_stage(sessions, stage_id);

    let Some(target) = pick_newest(&matches) else {
        bail!(
            "No live tmux session for stage '{stage_id}'. Live stage ids: {}",
            live_stage_ids(sessions)
        );
    };

    if matches.len() > 1 {
        println!(
            "Found {} live sessions for stage '{stage_id}'; attaching to the newest (session {})",
            matches.len(),
            target.id
        );
    }

    // Discovery already guaranteed `Some` for every session here.
    let tmux_session = tmux_session_name(target).unwrap_or_default();

    // Same precondition the overview applies, for the same reason and with
    // the same both-directions caveat — see `tmux_endpoint_ready`. Reported
    // before the TTY check so it reads as a diagnostic rather than as the
    // `exec` failing: without it, `exec` replaces this process and tmux's own
    // "no server running on ..." becomes the only thing the operator sees.
    if !tmux_endpoint_ready(target, &tmux_session) {
        bail!(
            "Stage '{stage_id}' has a live session ({}) whose tmux server is not accepting \
             clients — it is still spawning, or has just ended. Re-run in a moment.",
            target.id
        );
    }

    // Not `find_session_for_stage`: it returns the FIRST session file in
    // filesystem order without checking liveness. The live set above is correct.
    require_tty()?;

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
