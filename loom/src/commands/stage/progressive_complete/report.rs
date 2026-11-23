//! Operator-facing reports for a merge that was blocked or held by the
//! control-path gate.

use anyhow::Result;
use std::path::Path;

use crate::git::merge::MergeBlock;
use crate::models::stage::Stage;
use crate::verify::transitions::update_stage;

/// Print why the merge is blocked and who retries it. No merge runs while a
/// quarantine holds operator files, the daemon's retry included, so that
/// block gets its own remedy.
pub(in crate::commands::stage) fn report_merge_block(stage_id: &str, block: &MergeBlock) {
    for line in merge_block_lines(stage_id, block) {
        println!("{line}");
    }
}

/// The lines [`report_merge_block`] prints, in order.
fn merge_block_lines(stage_id: &str, block: &MergeBlock) -> Vec<String> {
    let mut lines = vec![format!("  Merge blocked: {block}")];
    if let MergeBlock::CheckoutRestoreIncomplete { quarantine, .. } = block {
        lines.push(format!(
            "  No merge of this stage runs while {} exists, not even the daemon's retry.",
            quarantine.display()
        ));
        if !block.states_its_remedy() {
            lines.push(format!(
                "  Move each file in it back to its path in the checkout, delete the directory, \
                 then run `loom stage merge {stage_id}` or let a running daemon retry it"
            ));
        }
        return lines;
    }
    lines.push(format!(
        "  A running daemon retries the merge every tick; otherwise run `loom stage merge {stage_id}`"
    ));
    lines
}

/// The error a completion ends with once its merge was blocked and reported
/// by [`report_merge_block`]. A quarantine that holds files keeps that
/// report's remedy: `loom stage retry` would drop the block that points at it.
pub(super) fn blocked_exit(stage: &Stage) -> String {
    let id = &stage.id;
    let head = format!(
        "Merge blocked for stage '{id}'.\nThe stage has been marked MergeBlocked.\n\
         This session should exit now."
    );
    if stage
        .merge
        .block
        .as_ref()
        .is_some_and(MergeBlock::holds_displaced_files)
    {
        return format!(
            "{head} Recover the quarantined files as reported above; do not run \
             loom stage retry {id}, which drops the block that points at the quarantine."
        );
    }
    format!("{head} Fix the issue and run: loom stage retry {id}")
}

/// Print why the control-path gate held the merge and how an operator
/// releases it.
pub(in crate::commands::stage) fn report_merge_hold(stage_id: &str, reason: &str) {
    println!("  Merge held: {reason}");
    println!(
        "  In .worktrees/{stage_id} review the change, then run \
         `loom stage human-review {stage_id} --force-complete`"
    );
}

/// Route `stage_id` to `NeedsHumanReview` on the fresh on-disk stage and
/// report the hold. For the commands that hold a branch without the progressive
/// merge's captured commit.
pub(in crate::commands::stage) fn route_held_to_review(
    stage_id: &str,
    work_dir: &Path,
    reason: &str,
) -> Result<()> {
    update_stage(stage_id, work_dir, |s| {
        s.route_to_review(reason);
        Ok(())
    })?;
    report_merge_hold(stage_id, reason);
    Ok(())
}

#[cfg(test)]
mod tests;
