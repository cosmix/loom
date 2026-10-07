//! Telling the operator a stage is waiting on their judgement.
//!
//! The terminal line and the desktop notification are one act, not two: a
//! stage that stops for a human is only stopped usefully if the human hears
//! about it, and the daemon may well be running behind another window. Kept
//! together so neither can be sent without the other.
//!
//! Both carry the reason's one-line headline: a review reason may end in the
//! pane text of a stalled session, which an agent controls, and that stays in
//! the stage record.

use colored::Colorize;

use super::super::clear_status_line;
use super::loop_recovery::review_headline;

pub(super) fn announce_needs_human_review(stage_id: &str, review_reason: Option<&str>) {
    clear_status_line();
    let headline = review_reason.map(review_headline);
    eprintln!(
        "{} Stage '{}' needs human review: {}",
        "REVIEW NEEDED:".magenta().bold(),
        stage_id,
        headline.as_deref().unwrap_or("No reason provided")
    );
    eprintln!("    Next: loom stage human-review {stage_id}");
    crate::orchestrator::notify::notify_needs_human_review(stage_id, headline.as_deref());
}
