//! Telling the operator a stage is waiting on their judgement.
//!
//! The terminal line and the desktop notification are one act, not two: a
//! stage that stops for a human is only stopped usefully if the human hears
//! about it, and the daemon may well be running behind another window. Kept
//! together so neither can be sent without the other.

use colored::Colorize;

use super::super::clear_status_line;

pub(super) fn announce_needs_human_review(stage_id: &str, review_reason: Option<&str>) {
    clear_status_line();
    let reason_str = review_reason.unwrap_or("No reason provided");
    eprintln!(
        "{} Stage '{}' needs human review: {}",
        "REVIEW NEEDED:".magenta().bold(),
        stage_id,
        reason_str
    );
    eprintln!("    Next: loom stage human-review {stage_id}");
    crate::orchestrator::notify::notify_needs_human_review(stage_id, review_reason);
}
