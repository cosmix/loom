//! Operator adapter for shared completion checks.

use crate::verify::completion::OutputStream;
pub(crate) use crate::verify::completion::VerificationChecks;

pub(super) fn print_event(stream: OutputStream, line: &str) {
    match stream {
        OutputStream::Stdout => println!("{line}"),
        OutputStream::Stderr => eprintln!("{line}"),
    }
}

/// Stop at the first failing check and show operator guidance for goal gaps.
pub(super) fn run(checks: &VerificationChecks<'_>) -> anyhow::Result<()> {
    crate::verify::completion::run_checks(checks, &mut |label, checked| {
        if label == "goal-backward" && checked.is_err() {
            crate::commands::stage::acceptance_runner::print_acceptance_failure_guidance(
                checks.stage_id,
            );
        }
        checked
    })
}

#[cfg(test)]
mod complete_verification_v2;
