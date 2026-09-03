//! Unit tests for how a crashed session is read.
//!
//! Included from `crash_classification.rs` via `#[path]`, matching how
//! `crash_handler_identity_tests.rs` is attached to `crash_handler.rs`.

use super::*;
use crate::orchestrator::retry::should_auto_retry;
use tempfile::TempDir;

/// The refusal that motivated all of this, copied from a real WSL run: loom's
/// generated settings set `sandbox.failIfUnavailable`, the Linux sandbox had no
/// `bwrap`/`socat` to build itself from, and claude exited 1 on the spot.
const SANDBOX_REFUSAL: &str = "Error: sandbox required but unavailable: sandbox is enabled but \
dependencies are missing: bubblewrap (bwrap) not installed, socat not installed · install \
missing tools (e.g. apt install bubblewrap socat) or see https://code.claude.com/docs/en/sandboxing\n\
  sandbox.failIfUnavailable is set — refusing to start without a working sandbox.\n";

fn session_with(work_dir: &Path, stderr: Option<&str>) -> Session {
    let mut session = Session::new();
    session.pid = Some(4242);
    session.tracking_key = "loom-build-api".to_string();
    if let Some(text) = stderr {
        let log = stderr_log_path(work_dir, &session.id);
        std::fs::create_dir_all(log.parent().unwrap()).unwrap();
        std::fs::write(&log, text).unwrap();
    }
    session
}

#[test]
fn fast_fail_fallback_applies_to_every_backend_not_just_native() {
    // THE regression: this was gated on `backend == Native`, so on the
    // tmux lane a `--remote-control` claude rejects crashed at startup,
    // the retry re-spawned with identical flags, and the stage burned its
    // entire attempt budget without the marker ever being written.
    // Backend is not an input here precisely because it must not be one.
    assert!(
        is_remote_control_fast_fail(1, true),
        "a fast crash with a verified pid must trigger the fallback on any backend"
    );
}

#[test]
fn a_session_that_never_reached_a_pid_is_a_hosting_failure_not_a_flag_rejection() {
    // What the old backend check was reaching for. A hosting failure must
    // not disable Remote Control for the rest of the run, and the honest
    // signal is the absence of a verified process — not which lane
    // spawned it.
    assert!(
        !is_remote_control_fast_fail(1, false),
        "no verified pid means hosting failed; Remote Control must not be blamed"
    );
}

#[test]
fn a_crash_outside_the_window_is_an_ordinary_failure() {
    // The window separates "the flag was rejected at startup" from "the
    // agent ran, then died". Without it every late crash would silently
    // disable Remote Control for the rest of the run.
    assert!(!is_remote_control_fast_fail(
        FAST_FAIL_WINDOW_SECS + 1,
        true
    ));
    assert!(
        is_remote_control_fast_fail(FAST_FAIL_WINDOW_SECS, true),
        "the boundary itself is inside the window"
    );
}

#[test]
fn a_fast_crash_from_a_verified_process_is_a_startup_refusal() {
    assert!(is_startup_refusal(1, true, false));
    assert!(
        is_startup_refusal(FAST_FAIL_WINDOW_SECS, true, false),
        "the boundary itself is inside the window"
    );
}

/// The one fast crash that is still worth retrying: the remote-control
/// fallback just fired, so the next spawn omits `--remote-control` and is not
/// the same command that died.
#[test]
fn a_crash_that_triggered_the_remote_control_fallback_is_not_a_refusal() {
    assert!(!is_startup_refusal(1, true, true));
}

#[test]
fn a_slow_crash_or_an_unverified_process_is_not_a_refusal() {
    // The agent ran for a while, then died: an ordinary crash, retryable.
    assert!(!is_startup_refusal(FAST_FAIL_WINDOW_SECS + 1, true, false));
    // No verified process means hosting failed, not that claude refused.
    assert!(!is_startup_refusal(1, false, false));
}

#[test]
fn a_refused_sandbox_is_named_as_one() {
    assert_eq!(
        classify_startup_refusal(Some(SANDBOX_REFUSAL)),
        FailureType::SandboxSetupFailure
    );
    assert_eq!(
        classify_startup_refusal(Some("Sandboxing requires WSL2, but this is WSL1")),
        FailureType::SandboxSetupFailure
    );
}

#[test]
fn an_unrecognized_refusal_stays_generic() {
    assert_eq!(
        classify_startup_refusal(Some("Invalid API key · Please run /login")),
        FailureType::StartupRefusal
    );
    assert_eq!(classify_startup_refusal(None), FailureType::StartupRefusal);
}

/// The contract the whole change rests on: neither refusal variant is
/// auto-retryable, so a deterministic exit can never consume a stage's attempt
/// budget three times over.
#[test]
fn no_refusal_variant_is_ever_auto_retried() {
    assert!(!should_auto_retry(&FailureType::StartupRefusal, 0, 3));
    assert!(!should_auto_retry(&FailureType::SandboxSetupFailure, 0, 3));
}

#[test]
fn a_refusal_carries_claudes_own_words_into_the_stage() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path();
    let session = session_with(work_dir, Some(SANDBOX_REFUSAL));
    let crash_report = work_dir
        .join("crashes")
        .join("20260903-120000-build-api.md");

    let classification = startup_refusal_crash(work_dir, &session, Some(&crash_report));

    assert_eq!(
        classification.failure_type,
        FailureType::SandboxSetupFailure
    );
    // Evidence is the reason followed by the captured lines, so `loom status`
    // shows the refusal itself rather than "session crashed".
    assert_eq!(classification.evidence.len(), 3);
    assert_eq!(classification.evidence[0], classification.reason);
    assert!(
        classification.evidence[1].contains("sandbox required but unavailable"),
        "{:?}",
        classification.evidence
    );
    assert!(
        classification.evidence[2].contains("failIfUnavailable"),
        "{:?}",
        classification.evidence
    );

    assert!(classification.reason.contains("before doing any work"));
    assert!(classification
        .reason
        .contains("20260903-120000-build-api.md"));
    assert!(classification.reason.contains("Reproduce by hand: bash "));

    let note = classification.console_note.unwrap();
    assert!(note.contains("will not be retried"), "{note}");
    assert!(note.contains("failIfUnavailable"), "{note}");
    assert!(note.contains("Crash report: "), "{note}");
    assert!(
        note.contains(&format!(
            "Reproduce: bash {}",
            wrapper_reproduce_path(work_dir, &session)
                .unwrap()
                .display()
        )),
        "{note}"
    );
}

/// The pane died before claude wrote anything, or the log never appeared. The
/// stage still blocks without retrying — it just cannot say why.
#[test]
fn a_refusal_without_captured_stderr_still_reports_and_still_blocks() {
    let temp = TempDir::new().unwrap();
    let session = session_with(temp.path(), None);

    let classification = startup_refusal_crash(temp.path(), &session, None);

    assert_eq!(classification.failure_type, FailureType::StartupRefusal);
    assert_eq!(classification.evidence, vec![classification.reason.clone()]);
    assert!(!classification.reason.contains("crash report"));
    let note = classification.console_note.unwrap();
    assert!(note.contains("no stderr captured"), "{note}");
}

/// A session with no tracking key was never assigned to a stage, so no wrapper
/// script bears its name and there is nothing to tell the operator to run.
#[test]
fn a_session_without_a_tracking_key_omits_the_reproduce_command() {
    let temp = TempDir::new().unwrap();
    let mut session = session_with(temp.path(), None);
    session.tracking_key = String::new();

    assert!(wrapper_reproduce_path(temp.path(), &session).is_none());
    let classification = startup_refusal_crash(temp.path(), &session, None);
    assert!(!classification.reason.contains("Reproduce"));
    assert!(!classification.console_note.unwrap().contains("Reproduce"));
}

#[test]
fn an_ordinary_crash_is_retryable_and_announces_nothing_extra() {
    let classification = ordinary_crash(Some(Path::new("/repo/.loom/work/crashes/report.md")));

    assert_eq!(classification.failure_type, FailureType::SessionCrash);
    assert!(classification.console_note.is_none());
    assert_eq!(classification.evidence, vec![classification.reason.clone()]);
    assert!(classification.reason.contains("report.md"));
}
