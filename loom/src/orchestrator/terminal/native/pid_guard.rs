//! PID-guarded liveness and teardown, shared by every session lane.
//!
//! The native lane, the tmux lane, the headless degradation inside
//! `SessionBackend`, and the `TmuxBackend`-less socket housekeeping in
//! [`crate::orchestrator::terminal::tmux`] all need the SAME two decisions:
//!
//! * *Is this session still alive?* Answered from the per-session PID file
//!   (with start-time verification, so a recycled OS PID is never mistaken for
//!   our session — O-14), and from `session.pid` only when that file is absent.
//! * *Which PID may we signal?* Only one a PID-file entry positively matches.
//!   `session.pid` is used only when there is NO PID-file evidence at all —
//!   never SIGTERM a stranger that inherited a dead session's PID.
//!
//! Those layers were copied verbatim into four call sites before this module
//! existed. Duplication of a rule is duplication of its bugs: a fix applied to
//! one copy leaves the other three wrong, and nothing in the type system says
//! so. conventions.md's dedup rule (a pattern at 3+ sites gets a canonical
//! home) is exactly this case.

use anyhow::{Context, Result};
use std::path::Path;

use crate::models::session::Session;

use super::{cleanup_stage_files, pid_matches_entry, read_pid_entry, NativeBackend};

/// Whether a liveness probe that has PROVEN a session dead may also delete
/// that session's tracking files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StalePidFiles {
    /// Remove the PID file and wrapper script. Used by the backend liveness
    /// paths, which own those files and run on the monitor's 5-second cadence,
    /// so leaving proven-stale entries behind would accumulate garbage and
    /// keep re-reading it.
    Reap,
    /// Leave every file untouched. Required by
    /// [`crate::orchestrator::terminal::tmux::socket_session_is_alive`], which
    /// `loom clean` / `loom init` call as a read-only judgment BEFORE deciding
    /// whether to destroy anything: a probe must not mutate the very evidence
    /// its caller is about to act on.
    Leave,
}

/// Whether `session` is alive, using ONLY PID evidence.
///
/// True when the per-session PID file positively matches a live process, or —
/// when there is NO PID-file evidence at all — when `session.pid` is still
/// alive. A PID file that exists but does not match is DECISIVE: it means the
/// process we recorded is gone (and, on Linux, that its PID now belongs to a
/// stranger), so the session is dead and `session.pid` is never consulted.
///
/// Deliberately never consults the window manager or `tmux has-session`: a
/// tmux server whose pane process has died still answers `has-session` with
/// exit 0, so admitting it as a liveness source would report a dead claude as
/// ALIVE, never file the crash, and never retry. Callers that have a genuinely
/// independent signal (the native lane's window-existence probe) layer it on
/// top of this, never inside it.
pub(crate) fn pid_only_is_alive(work_dir: &Path, session: &Session, stale: StalePidFiles) -> bool {
    if let Some((_, pid_key)) = NativeBackend::window_title_and_pid_key(session) {
        if let Some(entry) = read_pid_entry(work_dir, &pid_key) {
            if pid_matches_entry(&entry) {
                return true;
            }
            // PID file exists but the process is dead (or its PID was reused)
            // — the session is gone as far as this file is concerned.
            if stale == StalePidFiles::Reap {
                cleanup_stage_files(work_dir, &pid_key);
            }
            // ...and that verdict decides THIS call. Falling through to
            // `session.pid` here would resurrect exactly the process this
            // branch just rejected: `session.pid` was set from THIS file at spawn time —
            // `await_session_pid` reads it and the spawn passes the result to
            // `session.set_pid` — so it is the same number the entry carries.
            // (Its two other sources, `/proc` discovery constrained to this
            // session's `LOOM_SESSION_ID` and the native lane's terminal PID,
            // only run when this file held nothing live, and neither turns a
            // dead entry into a live claude.) On Linux the entry also carries
            // the recorded start-time, so "exists but does not match" means
            // that PID was RECYCLED by an unrelated process — and the
            // fall-through would report that stranger as our live session.
            // Worse, under `Reap` the file is now gone, so
            // `resolve_kill_target` no longer sees the entry that vetoes
            // signalling and takes its `None => session.pid` branch: liveness
            // says ALIVE, then `kill_session` SIGTERMs the stranger.
            //
            // Note what this does NOT close: `Reap` deletes the evidence, so a
            // LATER probe of the same session has no PID file and legitimately
            // falls back to `session.pid`. Callers that reap are expected to
            // act on the dead verdict now (the monitor persists `Crashed`)
            // rather than re-probe a session they already proved dead.
            return false;
        }
    }

    if let Some(pid) = session.pid {
        if crate::process::is_process_alive(pid) {
            return true;
        }
    }

    false
}

/// Which PID, if any, may be signalled for `session` — the O-14 rule, kept as
/// a pure function so it can be asserted on without sending a real signal.
///
/// The PID file is preferred over `session.pid` because it carries the
/// recorded process start-time: when the entry no longer matches, the PID was
/// recycled by an unrelated process and MUST NOT be signalled. A missing PID
/// file is the ONLY case that falls back to `session.pid`.
pub(crate) fn resolve_kill_target(work_dir: &Path, session: &Session) -> Option<u32> {
    let Some((_, pid_key)) = NativeBackend::window_title_and_pid_key(session) else {
        return session.pid;
    };
    match read_pid_entry(work_dir, &pid_key) {
        Some(entry) if pid_matches_entry(&entry) => Some(entry.pid),
        // PID file present but mismatched/dead → reused or gone; do not kill.
        Some(_) => None,
        // No PID file → fall back to the session's stored PID.
        None => session.pid,
    }
}

/// Terminate `session` from PID evidence alone, then clean up its tracking
/// files.
///
/// Tracking files are cleaned up whether or not anything was signalled, AND
/// whether or not the signal itself failed — a session that could not be
/// positively identified now will not become identifiable later, and leaving
/// its files behind only poisons the next session that reads them.
///
/// The error path matters, not just the "nothing to signal" path:
/// [`crate::process::terminate`] reports success for an already-dead process
/// (ESRCH) but errors on everything else, notably EPERM. That is reachable —
/// on macOS `process_start_time` is always `None`, so `pid_matches_entry`
/// degrades to a bare liveness probe and a recycled PID owned by another user
/// can match. Returning early there would strand this session's PID file and
/// wrapper forever, and every later teardown would re-read them and re-fail
/// identically. `cleanup_stage_files` is best-effort file removal, so running
/// it before propagating costs nothing.
pub(crate) fn pid_only_terminate(work_dir: &Path, session: &Session) -> Result<()> {
    let outcome = match resolve_kill_target(work_dir, session) {
        // Signal directly rather than shelling out to `kill(1)`: a syscall
        // cannot block, whereas a fork+exec on the orchestrator's poll thread
        // is one more way for teardown to stall scheduling. A process that is
        // already gone is a success, not a failure.
        Some(pid) => crate::process::terminate(pid)
            .map(|_| ())
            .with_context(|| format!("Failed to terminate session process {pid}")),
        None => Ok(()),
    };

    if let Some((_, pid_key)) = NativeBackend::window_title_and_pid_key(session) {
        cleanup_stage_files(work_dir, &pid_key);
    }

    outcome
}

#[cfg(test)]
mod tests {
    use super::super::pid_tracking::{create_pid_dir, create_wrapper_script, pid_file_path};
    use super::*;
    use tempfile::TempDir;

    /// A session whose PID file records a PID that cannot be alive, so the
    /// "entry present but not matching" branch is the one under test.
    ///
    /// The file carries the bare pid line and NO start-time line, so
    /// `pid_matches_entry` takes its `start_time: None` arm on every platform
    /// and decides on liveness alone — which for this PID is `false`
    /// everywhere, with no dependence on `/proc` or the host OS.
    fn session_with_dead_pid_file(work_dir: &Path) -> (Session, String) {
        let mut session = Session::new();
        session.assign_to_stage("stale-stage".to_string());
        let (_, pid_key) = NativeBackend::window_title_and_pid_key(&session).unwrap();
        create_pid_dir(work_dir).unwrap();
        std::fs::write(pid_file_path(work_dir, &pid_key), "999999999\n").unwrap();
        (session, pid_key)
    }

    #[test]
    fn reap_deletes_the_pid_file_of_a_proven_dead_session() {
        let temp = TempDir::new().unwrap();
        let (session, pid_key) = session_with_dead_pid_file(temp.path());

        assert!(!pid_only_is_alive(
            temp.path(),
            &session,
            StalePidFiles::Reap
        ));
        assert!(
            !pid_file_path(temp.path(), &pid_key).exists(),
            "Reap must clear the stale PID file so the monitor stops re-reading it"
        );
    }

    #[test]
    fn leave_keeps_the_pid_file_so_the_probe_stays_side_effect_free() {
        // `socket_session_is_alive` reports on evidence its caller is about to
        // act on destructively; deleting that evidence mid-judgment is exactly
        // what the `Leave` variant exists to forbid.
        let temp = TempDir::new().unwrap();
        let (session, pid_key) = session_with_dead_pid_file(temp.path());

        assert!(!pid_only_is_alive(
            temp.path(),
            &session,
            StalePidFiles::Leave
        ));
        assert!(
            pid_file_path(temp.path(), &pid_key).exists(),
            "Leave must not delete anything, even for a session it proved dead"
        );
    }

    #[test]
    fn a_mismatched_pid_entry_is_never_signalled() {
        // O-14, the security rule: the PID file names 999999999, which is
        // dead, so the recorded process is gone and whatever process happens
        // to own `session.pid` today is a STRANGER. Falling back to
        // `session.pid` here would SIGTERM it.
        let temp = TempDir::new().unwrap();
        let (mut session, _) = session_with_dead_pid_file(temp.path());
        session.pid = Some(std::process::id());

        assert_eq!(
            resolve_kill_target(temp.path(), &session),
            None,
            "a PID file that exists but does not match must veto the fallback, not defer to it"
        );
    }

    #[test]
    fn a_missing_pid_file_falls_back_to_the_session_pid() {
        // The one case with no PID-file evidence either way: nothing to
        // contradict the session's own record, so it is the best available.
        let temp = TempDir::new().unwrap();
        let mut session = Session::new();
        session.assign_to_stage("no-pid-file".to_string());
        session.pid = Some(4242);

        assert_eq!(resolve_kill_target(temp.path(), &session), Some(4242));
    }

    #[test]
    fn a_mismatched_pid_entry_reports_dead_instead_of_deferring_to_the_session_pid() {
        // THE REGRESSION THIS PINS, and it is the liveness half of the rule
        // `a_mismatched_pid_entry_is_never_signalled` pins for kills. With a
        // fall-through to `session.pid`, the two halves DISAGREE and chain
        // into a real SIGTERM on a stranger:
        //   1. the monitor calls this with `Reap`: the entry mismatches, the
        //      PID file is deleted, and the fall-through finds the recycled
        //      PID alive → reports ALIVE;
        //   2. `kill_session` → `resolve_kill_target` now finds NO PID file
        //      (step 1 deleted it), so its veto cannot fire and it takes the
        //      `None => session.pid` branch → terminates the stranger.
        // In production `session.pid` IS the entry's pid (both come from this
        // file via `await_session_pid` — bar the native lane's terminal-PID
        // fallback, which is a terminal, not a claude, and is precisely what
        // that lane's window probe layers on top). So a live-but-recycled PID
        // satisfies both. The fixture separates the two numbers only because a
        // start-time mismatch cannot be forged on macOS, where
        // `process_start_time` is always `None`.
        for stale in [StalePidFiles::Reap, StalePidFiles::Leave] {
            let temp = TempDir::new().unwrap();
            let (mut session, _) = session_with_dead_pid_file(temp.path());
            session.pid = Some(std::process::id());

            assert!(
                !pid_only_is_alive(temp.path(), &session, stale),
                "{stale:?}: a PID file that exists but does not match is decisive — \
                 `session.pid` must never be consulted to overturn it"
            );
        }
    }

    #[test]
    fn tracking_files_are_cleaned_up_even_when_the_signal_fails() {
        // `pid_only_terminate` promises cleanup "whether or not anything was
        // signalled"; a `?` on the terminate call broke that promise for the
        // case that needs it most. `crate::process::terminate` succeeds on an
        // already-dead process (ESRCH) but errors on EPERM — reachable on
        // macOS, where `pid_matches_entry` degrades to bare liveness and a
        // recycled PID owned by ANOTHER USER can match. The files would then
        // survive forever and every later kill would re-error identically.
        //
        // The failure is forced here with an out-of-range PID rather than a
        // real EPERM: `terminate` rejects it before it can signal ANY process,
        // which is both deterministic on every platform and the only way to
        // exercise this path in a test without aiming SIGTERM at a process the
        // test does not own.
        let temp = TempDir::new().unwrap();
        let mut session = Session::new();
        session.assign_to_stage("unsignallable-stage".to_string());
        session.pid = Some(u32::MAX);
        let (_, pid_key) = NativeBackend::window_title_and_pid_key(&session).unwrap();
        // No PID file: `resolve_kill_target` then falls back to `session.pid`,
        // which is what makes `terminate` run at all. The wrapper script is
        // the observable half of the tracking files.
        let wrapper = create_wrapper_script(
            temp.path(),
            &pid_key,
            "unsignallable-stage",
            &session.id,
            "claude 'prompt'",
            None,
        )
        .unwrap();
        assert!(
            wrapper.exists(),
            "positive control: there must be a tracking file to survive before \
             the assertion below can mean anything"
        );

        let err = pid_only_terminate(temp.path(), &session)
            .expect_err("an unsignallable PID must still surface as an error");

        assert!(
            !wrapper.exists(),
            "the wrapper script must be cleaned up on the error path too, \
             or every later teardown re-reads it and re-fails: {err}"
        );
    }

    #[test]
    fn a_matching_pid_entry_wins_over_the_session_pid() {
        // Our own live PID with no recorded start-time matches, and it must be
        // preferred over the session's stale record — the PID file is the more
        // current source: the wrapper writes it immediately before `exec`, and
        // `exec` preserves the PID, so the file names the claude process
        // itself.
        let temp = TempDir::new().unwrap();
        let mut session = Session::new();
        session.assign_to_stage("live-stage".to_string());
        session.pid = Some(4242);
        let (_, pid_key) = NativeBackend::window_title_and_pid_key(&session).unwrap();
        create_pid_dir(temp.path()).unwrap();
        std::fs::write(
            pid_file_path(temp.path(), &pid_key),
            format!("{}\n", std::process::id()),
        )
        .unwrap();

        assert_eq!(
            resolve_kill_target(temp.path(), &session),
            Some(std::process::id())
        );
    }
}
