//! Identity and discovery for the tiled overview viewer.
//!
//! This module owns the IDENTITY half of the per-repo overview viewer (its
//! socket name, its tmux session name, the argv for a single pane's nested
//! attach command) and the DISCOVERY half (which sessions are live and
//! attachable, in what order). It is shared by two callers with different
//! lifecycles over the same viewer: `commands::attach` builds the viewer once
//! and `exec`s into it, while `tmux::reconcile` keeps an already-built viewer
//! in sync while the daemon runs. The ARGV BUILDER that assembles the initial
//! `tmux` invocation sequence (`build_overview_argv` and friends) stays in
//! `commands/attach/overview.rs` — it runs once, at build time, and has no
//! reconciliation counterpart. Only identity and discovery are shared here,
//! so the two surfaces cannot drift on who is attachable or what socket to
//! use.

use anyhow::Result;
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use crate::models::session::{Session, SessionBackendKind, SessionStatus};
use crate::orchestrator::terminal::native::NativeBackend;
use crate::parser::frontmatter::parse_from_markdown;

use super::{socket_name, socket_path_for, TmuxBackend};

/// tmux session name of the tiled viewer. Fixed; the SOCKET is what varies per
/// repo. Shared by `commands/attach`, which builds the viewer under this
/// name, and `tmux::reconcile`, which targets the same name to keep it in
/// sync.
pub(crate) const OVERVIEW_SESSION: &str = "loom-overview";

/// Per-REPOSITORY viewer socket name, `loom-view-<8 hex>`. The tmux socket
/// directory is per-USER, not per-repo, so a fixed global name would make two
/// checkouts collide — and the overview's own best-effort `kill-session`
/// would then tear down the other repo's viewer. Shared by `commands/attach`,
/// which derives the socket to build the viewer on, and `tmux::reconcile`,
/// which derives the same socket to find the viewer it maintains.
pub(crate) fn viewer_socket_name(work_dir: &Path) -> String {
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
/// `build_overview_argv` in `commands/attach/overview.rs`), never handed to
/// tmux's own `default-shell`. `unset` is not portable: under
/// `default-shell=/bin/csh` (verified on tmux 3.7b) `unset` clears a shell
/// variable, never the environment, so the nested attach still sees `$TMUX`
/// and refuses it; fish has no `unset` at all. A guaranteed POSIX `sh`
/// sidesteps the operator's login shell entirely, and makes `escape_arg`'s
/// `sh`-flavoured quoting provably correct rather than accidentally so.
/// Shared by `commands/attach`, which emits this as the initial pane command,
/// and `tmux::reconcile`, which emits it again for every pane it adds later.
pub(crate) fn pane_command(session_socket: &str, tmux_session: &str) -> String {
    format!(
        "unset TMUX; exec tmux -L {} attach-session -t {}",
        escape_arg(session_socket),
        escape_arg(tmux_session)
    )
}

/// `(session_socket, tmux_session)` for every session that can actually be
/// attached to, in discovery order.
///
/// `ready` is injected so the selection rule is testable without a tmux
/// server — the same split
/// [`crate::orchestrator::terminal::tmux::evaluate_new_session`] uses to keep
/// an untestable decision out of an untestable context. Production always
/// passes [`endpoint_ready`]. Shared by `commands/attach`, which uses this to
/// build the viewer's initial panes, and `tmux::reconcile`, which uses it to
/// decide which panes to add or drop while the daemon runs.
///
/// A session is admitted only when `ready` says so AND both values it would
/// contribute to a pane command round-trip unambiguously
/// ([`is_plain_identifier`]). That check lives here rather than in either
/// caller because this is the single point that PRODUCES panes for both of
/// them — checking once here is the only way neither can drift out of sync
/// with the other.
pub(crate) fn attachable_panes(
    sessions: &[Session],
    ready: impl Fn(&Session, &str) -> bool,
) -> Vec<(String, String)> {
    sessions
        .iter()
        .filter_map(|session| {
            // Discovery already guaranteed a resolvable name for every
            // session here, so this `?` never actually fires.
            let tmux_session = tmux_session_name(session)?;
            let socket = socket_name(session);
            let attachable = ready(session, &tmux_session)
                && is_plain_identifier(&socket)
                && is_plain_identifier(&tmux_session);
            attachable.then_some((socket, tmux_session))
        })
        .collect()
}

/// Whether `value` survives the pane-command round trip unambiguously.
///
/// True iff `value` is non-empty and every character satisfies
/// `c.is_ascii_alphanumeric() || c == '-' || c == '_'` — exactly the
/// charset [`crate::validation::validate_id`] (`validation.rs:64-66`)
/// already enforces for every loom id. Nothing this guard rejects is
/// something loom's own id generation can legitimately produce: session ids
/// are `session-<uuid8>-<unixts>` (`Session::generate_id`,
/// `models/session/methods.rs`) and tracking keys are
/// `loom-[<kind>-]<stage_id>` (`Session::derive_tracking_key`, same file).
/// A future reader should not treat the charset as arbitrary and loosen it
/// without widening `validate_id` first.
///
/// This is NOT the injection defence — `escape_arg` neutralises shell
/// metacharacters and stays exactly as it is; neither guard makes the other
/// redundant. What this one guards against is the RECONCILER'S ROUND TRIP:
/// `tmux::reconcile` re-derives a session's socket by parsing `list-panes`'
/// recorded start command back apart, tokenising on whitespace. A session
/// id or tracking key containing a space makes `escape_arg` shell-quote the
/// socket (e.g. `'loom-a b'`), so the reconciler reads back the truncated
/// `loom-a`, which never matches the real socket — it concludes that
/// session has no pane and emits an ADD, every tick, forever, until
/// `split-window` fails for lack of space and (since the executor stops at
/// the first failed step) poisons every other session's reconciliation for
/// the rest of that pass. A newline in an identifier is worse: the pane's
/// start command then spans two lines of `list-panes` output and misaligns
/// the fields of every pane listed after it.
///
/// A session that fails this check is silently absent from the viewer
/// rather than rendered — a deliberate trade, since the alternative is a
/// viewer that degrades for every OTHER session too.
fn is_plain_identifier(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
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
pub(crate) fn tmux_session_name(session: &Session) -> Option<String> {
    NativeBackend::window_title_and_pid_key(session).map(|(title, _)| title)
}

/// Every live tmux-hosted session recorded in `<work_dir>/sessions`, oldest
/// first. Shared by `commands/attach`, which uses this as its discovery set
/// for both direct attach and the overview, and `tmux::reconcile`, which
/// re-runs it on each pass to notice sessions that appeared or disappeared.
pub(crate) fn live_tmux_sessions(work_dir: &Path) -> Result<Vec<Session>> {
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
/// what took the whole viewer down before `VIEWER_HARDENING`
/// (`commands/attach/overview.rs`) existed.
///
/// # Bounded, because the daemon calls this too
///
/// This probe is called from `tmux::reconcile`, which runs on the daemon's
/// single scheduler loop — an unbounded subprocess here would wedge the whole
/// orchestrator, the same reason every native terminal probe carries a
/// two-second deadline (see `architecture/terminal-backends.md`). The socket
/// existence check stays a cheap filesystem stat ahead of it; only the
/// `has-session` probe itself needs a deadline.
///
/// So this is an ADDITIONAL precondition on the attach path and on the viewer
/// reconciler that maintains it — the only two callers permitted. It never
/// substitutes for `is_session_alive`, and the monitor must never call it:
/// consulting a tmux server for liveness would report a dead agent as alive
/// and the crash would never be filed.
pub(crate) fn endpoint_ready(session: &Session, tmux_session: &str) -> bool {
    let socket = socket_name(session);
    if !socket_path_for(&socket).exists() {
        return false;
    }
    super::run_tmux_control(
        &["-L", &socket, "has-session", "-t", tmux_session],
        super::TMUX_PROBE_TIMEOUT,
        format!("tmux has-session ({socket})"),
    )
    .is_ok_and(|probe| probe.status.success())
}

/// Every attachable session as `(session_socket, tmux_session)`, in the same
/// oldest-first order `loom attach` builds its panes in.
pub(crate) fn attachable_sessions(work_dir: &Path) -> Result<Vec<(String, String)>> {
    let sessions = live_tmux_sessions(work_dir)?;
    Ok(attachable_panes(&sessions, endpoint_ready))
}

// `pub(crate)` (not `pub(super)`) so `commands/attach/tests.rs` can import
// `stub_session` from here rather than keeping a second copy — mirrors
// `tmux/socket.rs`'s `TmuxTmpDirGuard`, shared with `tmux::tests` the same way.
#[cfg(test)]
#[path = "tests_viewer.rs"]
pub(crate) mod tests;
#[cfg(test)]
#[path = "tests_discovery.rs"]
mod tests_discovery;
