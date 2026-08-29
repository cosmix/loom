//! Reconciling the daemon's `TMUX_TMPDIR` with `loom attach`'s.
//!
//! The tmux socket directory is `$TMUX_TMPDIR` (or `/tmp` when unset) joined
//! with `tmux-<uid>` (see `orchestrator::terminal::tmux::socket::loom_socket_dir`),
//! resolved from the CALLING process's own environment. The daemon's
//! environment is captured once at `loom run` time and frozen after the fork
//! (`daemon::server::environment::DaemonEnvironment`, which keeps
//! `TMUX_TMPDIR` on its allowlist); `loom attach` instead resolves
//! `TMUX_TMPDIR` fresh from the operator's current shell. If the two differ —
//! say, the daemon was started from a shell with `TMUX_TMPDIR` set (or
//! unset) differently than the shell `loom attach` runs from — `loom attach`
//! looks in a directory the daemon's tmux servers never wrote to and sees no
//! live sessions.
//!
//! This module lets a running orchestrator record what it actually resolved
//! ([`record_tmux_tmpdir`]/[`record_tmux_tmpdir_best_effort`]), remove that
//! record on exit ([`remove_tmux_tmpdir_record`]), and lets `loom attach`
//! adopt a still-live record into its OWN process environment before doing
//! any tmux discovery ([`adopt_recorded_tmux_tmpdir`]), so the two processes
//! agree by construction instead of by coincidence — and a record left by an
//! orchestrator that has since exited is never mistaken for a live one (see
//! the daemon-liveness gate on [`adopt_recorded_tmux_tmpdir`]).

use std::ffi::OsString;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::Path;

use crate::daemon::DaemonServer;

/// File name of the recorded value under `.work/`.
const RECORD_FILE: &str = "tmux-tmpdir";

/// Write `.work/tmux-tmpdir` from the calling process's own environment.
///
/// The file holds the raw bytes of the value (no encoding assumed — a
/// non-UTF-8 `TMUX_TMPDIR` round-trips unchanged), or is left EMPTY when
/// `TMUX_TMPDIR` is unset. That distinction matters to
/// [`adopt_recorded_tmux_tmpdir`], which must tell "recorded as unset" apart
/// from "no record at all".
pub fn record_tmux_tmpdir(work_dir: &Path) -> std::io::Result<()> {
    let bytes = std::env::var_os("TMUX_TMPDIR")
        .map(|value| value.as_bytes().to_vec())
        .unwrap_or_default();
    std::fs::write(work_dir.join(RECORD_FILE), bytes)
}

/// Best-effort [`record_tmux_tmpdir`] for callers (daemon startup, foreground
/// `loom run`) that must never fail their own startup over a missing record:
/// a write failure only degrades `loom attach`'s directory-guessing, never
/// orchestration itself.
pub fn record_tmux_tmpdir_best_effort(work_dir: &Path) {
    if let Err(e) = record_tmux_tmpdir(work_dir) {
        eprintln!("Warning: could not record tmux socket dir: {e}");
    }
}

/// Remove `.work/tmux-tmpdir` so a `loom attach` run after this orchestrator
/// has exited never finds a record to adopt. Defense in depth alongside the
/// daemon-liveness gate on [`adopt_recorded_tmux_tmpdir`] — that gate alone
/// already refuses a record once no daemon is alive, but deleting the file
/// also keeps a `.work/` directory from carrying dead state indefinitely.
/// Best-effort: a missing file is not an error; any other failure is
/// reported but never propagated, since cleanup must not block shutdown.
pub fn remove_tmux_tmpdir_record(work_dir: &Path) {
    if let Err(e) = std::fs::remove_file(work_dir.join(RECORD_FILE)) {
        if e.kind() != std::io::ErrorKind::NotFound {
            eprintln!("Warning: could not remove tmux socket dir record: {e}");
        }
    }
}

/// What [`adopt_recorded_tmux_tmpdir`] did, for the caller to report.
#[derive(Debug, PartialEq, Eq)]
pub enum TmuxTmpdirAdoption {
    /// No daemon is alive for this `.work/` (per [`DaemonServer::is_running`]).
    /// Any record present is presumed stale — left behind by an orchestrator
    /// that has since exited — so it is never adopted and the process env is
    /// left untouched.
    DaemonNotRunning,
    /// No `.work/tmux-tmpdir` record exists (daemon predates this feature,
    /// or has not recorded yet) — nothing to adopt.
    NoRecord,
    /// The recorded value already matches this process's ambient
    /// `TMUX_TMPDIR` (both present and equal, or both absent) — no change.
    AlreadyMatching,
    /// The recorded value differed from the ambient one; this process's
    /// `TMUX_TMPDIR` was set (or unset) to match the daemon's.
    Adopted {
        recorded: Option<OsString>,
        ambient: Option<OsString>,
    },
}

/// Pure decision, isolated from the env mutation (and from the real
/// daemon-liveness check) so it is unit-testable without touching the
/// process environment or spawning any process. `recorded` is `None` when no
/// record file exists at all, `Some(None)` when the record exists but is
/// empty/whitespace-only (recorded as unset), and `Some(Some(value))`
/// otherwise.
fn decide(
    daemon_alive: bool,
    recorded: Option<Option<OsString>>,
    ambient: Option<OsString>,
) -> TmuxTmpdirAdoption {
    if !daemon_alive {
        return TmuxTmpdirAdoption::DaemonNotRunning;
    }
    let Some(recorded) = recorded else {
        return TmuxTmpdirAdoption::NoRecord;
    };
    if recorded == ambient {
        return TmuxTmpdirAdoption::AlreadyMatching;
    }
    TmuxTmpdirAdoption::Adopted { recorded, ambient }
}

/// Trim exactly one trailing line ending (`\n` or `\r\n`) from a raw record.
/// Operates byte-wise, never via `str`, so a non-UTF-8 `TMUX_TMPDIR` value
/// round-trips unchanged instead of being corrupted or silently dropped.
fn trim_trailing_newline(bytes: &[u8]) -> &[u8] {
    let bytes = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    bytes.strip_suffix(b"\r").unwrap_or(bytes)
}

/// Read `.work/tmux-tmpdir` as raw bytes, trimming a trailing line ending. A
/// record that is empty or all-whitespace after trimming means "recorded as
/// unset".
fn read_recorded(work_dir: &Path) -> Option<Option<OsString>> {
    let raw = std::fs::read(work_dir.join(RECORD_FILE)).ok()?;
    let trimmed = trim_trailing_newline(&raw);
    Some(if trimmed.iter().all(u8::is_ascii_whitespace) {
        None
    } else {
        Some(OsString::from_vec(trimmed.to_vec()))
    })
}

/// While an orchestrator is alive for this `.work/` (per
/// [`DaemonServer::is_running`]), read `.work/tmux-tmpdir`; if it exists and
/// differs from this process's `TMUX_TMPDIR` (present vs absent counts as
/// differing), set/unset `TMUX_TMPDIR` in THIS process so every later
/// `loom_socket_dir()` call and every tmux subprocess this process spawns
/// resolves the daemon's directory. With no live daemon, any record present
/// is presumed stale and is never adopted — see
/// [`TmuxTmpdirAdoption::DaemonNotRunning`].
pub fn adopt_recorded_tmux_tmpdir(work_dir: &Path) -> TmuxTmpdirAdoption {
    let daemon_alive = DaemonServer::is_running(work_dir);
    let recorded = read_recorded(work_dir);
    let ambient = std::env::var_os("TMUX_TMPDIR");
    let decision = decide(daemon_alive, recorded, ambient);

    if let TmuxTmpdirAdoption::Adopted { recorded, .. } = &decision {
        match recorded {
            Some(value) => std::env::set_var("TMUX_TMPDIR", value),
            None => std::env::remove_var("TMUX_TMPDIR"),
        }
    }

    decision
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use tempfile::TempDir;

    // ----- Pure decision logic (no process env, no real daemon) -----

    #[test]
    fn no_daemon_alive_always_wins_regardless_of_record_or_ambient() {
        assert_eq!(
            decide(false, None, None),
            TmuxTmpdirAdoption::DaemonNotRunning
        );
        assert_eq!(
            decide(false, Some(Some(OsString::from("/recorded"))), None),
            TmuxTmpdirAdoption::DaemonNotRunning
        );
        assert_eq!(
            decide(
                false,
                Some(Some(OsString::from("/recorded"))),
                Some(OsString::from("/ambient"))
            ),
            TmuxTmpdirAdoption::DaemonNotRunning
        );
    }

    #[test]
    fn no_record_file() {
        assert_eq!(decide(true, None, None), TmuxTmpdirAdoption::NoRecord);
        assert_eq!(
            decide(true, None, Some(OsString::from("/whatever"))),
            TmuxTmpdirAdoption::NoRecord
        );
    }

    #[test]
    fn empty_record_and_unset_ambient_match() {
        assert_eq!(
            decide(true, Some(None), None),
            TmuxTmpdirAdoption::AlreadyMatching
        );
    }

    #[test]
    fn empty_record_with_set_ambient_adopts_unset() {
        let ambient = Some(OsString::from("/tmp/ambient"));
        assert_eq!(
            decide(true, Some(None), ambient.clone()),
            TmuxTmpdirAdoption::Adopted {
                recorded: None,
                ambient,
            }
        );
    }

    #[test]
    fn recorded_value_with_unset_ambient_adopts_set() {
        let recorded = Some(OsString::from("/tmp/daemon"));
        assert_eq!(
            decide(true, Some(recorded.clone()), None),
            TmuxTmpdirAdoption::Adopted {
                recorded,
                ambient: None,
            }
        );
    }

    #[test]
    fn recorded_value_matching_ambient() {
        let value = Some(OsString::from("/tmp/same"));
        assert_eq!(
            decide(true, Some(value.clone()), value),
            TmuxTmpdirAdoption::AlreadyMatching
        );
    }

    #[test]
    fn recorded_value_differing_from_ambient_adopts() {
        let recorded = Some(OsString::from("/tmp/daemon"));
        let ambient = Some(OsString::from("/tmp/shell"));
        assert_eq!(
            decide(true, Some(recorded.clone()), ambient.clone()),
            TmuxTmpdirAdoption::Adopted { recorded, ambient }
        );
    }

    // ----- read_recorded / trim_trailing_newline byte-safety -----

    #[test]
    fn read_recorded_trims_only_one_trailing_line_ending() {
        let work = TempDir::new().unwrap();
        std::fs::write(work.path().join(RECORD_FILE), b"/tmp/daemon\r\n").unwrap();
        assert_eq!(
            read_recorded(work.path()),
            Some(Some(OsString::from("/tmp/daemon")))
        );
    }

    #[test]
    fn read_recorded_treats_whitespace_only_record_as_unset() {
        let work = TempDir::new().unwrap();
        std::fs::write(work.path().join(RECORD_FILE), b"   \n").unwrap();
        assert_eq!(read_recorded(work.path()), Some(None));
    }

    #[test]
    #[serial]
    fn record_then_read_round_trips_non_utf8_bytes() {
        let _guard = TmuxAmbientEnvGuard::capture();
        let work = TempDir::new().unwrap();
        // 0xFF is not valid UTF-8 in this position; a `str`-based read would
        // either corrupt it (lossy) or drop it (`read_to_string` erroring).
        let non_utf8 = OsString::from_vec(vec![b'/', b't', b'm', b'p', b'/', 0xFF]);
        std::env::set_var("TMUX_TMPDIR", &non_utf8);
        record_tmux_tmpdir(work.path()).unwrap();
        assert_eq!(read_recorded(work.path()), Some(Some(non_utf8)));
    }

    // ----- adopt_recorded_tmux_tmpdir: real daemon-liveness gate -----
    //
    // No daemon lock is ever held in these tests, so `DaemonServer::is_running`
    // is always `false` here — exactly the scenario this gate exists for
    // (task A): a record left by an earlier run must not be adopted once
    // nothing is listening on it anymore.

    #[test]
    #[serial]
    fn adopt_with_no_live_daemon_ignores_any_record_and_leaves_env_untouched() {
        let _guard = TmuxAmbientEnvGuard::capture();
        let work = TempDir::new().unwrap();
        std::fs::write(work.path().join(RECORD_FILE), b"/some/recorded/dir").unwrap();
        std::env::set_var("TMUX_TMPDIR", "/attach/shell/dir");

        let adoption = adopt_recorded_tmux_tmpdir(work.path());

        assert_eq!(adoption, TmuxTmpdirAdoption::DaemonNotRunning);
        assert_eq!(
            std::env::var_os("TMUX_TMPDIR"),
            Some(OsString::from("/attach/shell/dir")),
            "the ambient value must be left exactly as this process set it"
        );
    }

    #[test]
    #[serial]
    fn adopt_without_any_record_file_and_no_live_daemon_is_still_daemon_not_running() {
        let _guard = TmuxAmbientEnvGuard::capture();
        let work = TempDir::new().unwrap();

        assert_eq!(
            adopt_recorded_tmux_tmpdir(work.path()),
            TmuxTmpdirAdoption::DaemonNotRunning,
            "the liveness gate is checked before the record is even consulted"
        );
    }

    /// Saves and restores the ambient `TMUX_TMPDIR` around a test, without
    /// setting a value of its own — unlike
    /// `orchestrator::terminal::tmux::socket::tests::TmuxTmpDirGuard`, which
    /// this module cannot reach: `tmux::mod.rs` declares `mod socket;`
    /// private, so only `tmux`'s own submodules can name
    /// `super::socket::tests::TmuxTmpDirGuard` (see its two other local
    /// duplicates at `commands::clean::sessions` and `commands::init::tests`
    /// for the same reason).
    struct TmuxAmbientEnvGuard {
        original: Option<OsString>,
    }

    impl TmuxAmbientEnvGuard {
        fn capture() -> Self {
            Self {
                original: std::env::var_os("TMUX_TMPDIR"),
            }
        }
    }

    impl Drop for TmuxAmbientEnvGuard {
        fn drop(&mut self) {
            match &self.original {
                Some(value) => std::env::set_var("TMUX_TMPDIR", value),
                None => std::env::remove_var("TMUX_TMPDIR"),
            }
        }
    }
}
