//! Remote Control fast-fail handling and the console line for a crash's outcome.

use crate::models::failure::FailureType;
use crate::models::stage::Stage;
use crate::orchestrator::retry::{crash_backoff, should_auto_retry};

use super::super::clear_status_line;
use super::super::crash_classification::{CrashClassification, FAST_FAIL_WINDOW_SECS};

/// If this crash is suspected to be a rejected `--remote-control` flag,
/// latch Remote Control off for the rest of this process (see
/// `remote_control::disable_for_this_process`) so the retry drops the flag
/// entirely. A no-op otherwise.
pub(super) fn maybe_disable_remote_control(remote_control_suspected: bool) {
    if remote_control_suspected {
        crate::remote_control::disable_for_this_process(&format!(
            "session exited within {FAST_FAIL_WINDOW_SECS}s of spawn with --remote-control"
        ));
    }
}

/// The stage-facing note for the crash that triggered the in-process Remote
/// Control disable — names the cause and that the stage will retry without
/// the flag. A no-op for every other crash.
pub(super) fn note_remote_control_disabled(
    remote_control_suspected: bool,
    sid: &str,
    classification: &mut CrashClassification,
) {
    if !remote_control_suspected {
        return;
    }
    let hint = format!(
        "session exited within {FAST_FAIL_WINDOW_SECS}s with --remote-control; Remote \
         Control disabled for the rest of this daemon run, retrying stage '{sid}' without it"
    );
    classification.reason.push_str(&format!("; {hint}"));
    classification.evidence.push(hint.clone());
    classification.console_note = Some(match classification.console_note.take() {
        Some(note) => format!("{note} {hint}"),
        None => hint,
    });
}

/// The console message for a crash's outcome: the retry countdown, the
/// startup-refusal note, or the "exhausted its attempts" message — whichever
/// applies.
pub(super) fn announce_crash_outcome(
    sid: &str,
    failure_type: &FailureType,
    updated: &Stage,
    console_note: Option<String>,
) {
    let max = updated.max_retries.unwrap_or(3);
    if should_auto_retry(failure_type, updated.retry_count, max) {
        let backoff = crash_backoff(updated.retry_count);
        clear_status_line();
        eprintln!(
            "Stage '{}' crashed (attempt {}/{}). Will retry in {}s...",
            sid,
            updated.retry_count,
            max,
            backoff.as_secs()
        );
    } else if let Some(note) = console_note {
        clear_status_line();
        eprintln!("Stage '{sid}': {note}");
    } else if updated.retry_count >= max {
        clear_status_line();
        eprintln!(
            "Stage '{}' failed after {} attempts. Fix the cause, then run `loom stage retry {}`.",
            sid, updated.retry_count, sid
        );
    }
}
