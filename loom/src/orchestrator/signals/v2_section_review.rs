//! The review blocks of the plan-version-2 signal section (DESIGN D12, D16).
//!
//! Standard and integration-verify stages get the review gate. Integration-verify
//! also gets the plan's pending reviewer suggestions, and knowledge-distill the
//! rule for the suggestions nobody implemented.

use std::path::Path;

use anyhow::Result;

use crate::fs::memory::{
    list_journals, read_journal, read_staged_entries, settled_ids, MemoryEntry, MemoryEntryType,
};
use crate::models::stage::{Stage, StageType};
use crate::verify::review::report::single_line;
use crate::verify::review::store::load_carried;

/// DESIGN D16 BLOCK-E, byte for byte. doctrine-v2 pins it against the
/// orchestration skill's copy.
const REVIEW_ORDER: &str = "**Review order (plan v2):** fix every finding, run the full gate, then run the final review round, then complete. Edit nothing after the final review round: any edit, formatting included, changes the change fingerprint and needs another round. A commit does not change it.";

/// The review gate of a v2 standard or integration-verify stage: what
/// `loom stage complete` checks, how reviews are run and briefed, the review
/// order, and the findings an earlier stage deferred to this one.
pub(super) fn append_review_gate(content: &mut String, stage: &Stage, work_dir: &Path) {
    if !matches!(
        stage.stage_type,
        StageType::Standard | StageType::IntegrationVerify
    ) {
        return;
    }
    content.push_str("\n## Review Gate\n\n");
    content.push_str(
        "`loom stage complete` fails until the latest review round matches the current state \
         of the worktree and no finding is open. Every finding blocks completion, whatever its \
         severity; suggestions never do.\n\n",
    );
    content.push_str(&format!(
        "- Spawn a `loom-code-reviewer` for the stage diff. Only that agent type is recorded: \
         when it stops, a hook records the `loom-review` block ending its final message as the \
         next review round. A final message without a valid block is recorded as malformed and \
         counts for nothing.\n\
         - Paste the output of `loom stage review status {id}` into every re-review brief. It \
         lists the rounds, every open finding with its id, whether the latest round matches the \
         current worktree, and the files changed since that round.\n\
         - A re-review covers the files changed since the previous round plus the open \
         findings. A finding closes when a later round lists its id under `resolved`.\n\n",
        id = stage.id
    ));
    content.push_str(REVIEW_ORDER);
    content.push('\n');
    append_carried_findings(content, &stage.id, work_dir);
}

/// The findings deferred to this stage from an earlier one, with their ids.
fn append_carried_findings(content: &mut String, stage_id: &str, work_dir: &Path) {
    let carried = match load_carried(work_dir, stage_id) {
        Ok(carried) => carried.carried,
        Err(error) => {
            content.push_str(&format!(
                "\nLoom could not read the findings carried into this stage ({}). \
                 `loom stage review status {stage_id}` lists every open finding.\n",
                single_line(&format!("{error:#}"))
            ));
            return;
        }
    };
    if carried.is_empty() {
        return;
    }
    content.push_str("\n### Carried Findings\n\n");
    content.push_str(
        "An earlier stage deferred these findings to this one. Each blocks completion like one \
         of your own until a review round lists its id under `resolved`.\n\n",
    );
    for item in &carried {
        let finding = &item.finding;
        content.push_str(&format!(
            "- `{}` ({}) `{}:{}`: {}\n",
            single_line(&item.id),
            single_line(&finding.severity),
            single_line(&finding.file),
            finding.line,
            single_line(&finding.claim),
        ));
    }
}

/// Integration-verify: every reviewer suggestion of the plan's stages that
/// has no receipt yet, with its id, and what to do with each one. Rendered
/// only when there is at least one, or when the journals cannot be read.
pub(super) fn append_reviewer_suggestions(content: &mut String, stage: &Stage, work_dir: &Path) {
    if stage.stage_type != StageType::IntegrationVerify {
        return;
    }
    let suggestions = pending_suggestions(work_dir);
    if matches!(&suggestions, Ok(found) if found.is_empty()) {
        return;
    }
    content.push_str("\n## Reviewer Suggestions\n\n");
    content.push_str(
        "Reviewers of this plan's stages left these suggestions; none of them blocked a stage. \
         Implement, defer or ignore each one. Resolve each one you implement with \
         `loom memory resolve <id> --outcome implemented --reason <what changed>`; leave the \
         rest pending.\n\n",
    );
    match suggestions {
        Ok(found) => {
            for (stage_id, entry) in &found {
                content.push_str(&format!(
                    "- `{}` ({stage_id}): {}\n",
                    entry.id,
                    single_line(&entry.content)
                ));
            }
        }
        Err(error) => content.push_str(&format!(
            "Loom could not read the plan's memory journals ({}). \
             `loom memory pending --group` lists the pending ones under `suggestions`.\n",
            single_line(&format!("{error:#}"))
        )),
    }
}

/// Knowledge-distill: the rule for the reviewer suggestions nobody implemented.
pub(super) fn append_unimplemented_suggestions(content: &mut String, stage: &Stage) {
    if stage.stage_type != StageType::KnowledgeDistill {
        return;
    }
    content.push_str(
        "\n## Unimplemented Suggestions\n\n\
         `loom memory pending --group` lists the reviewer suggestions nobody implemented under \
         `suggestions`. Record every one in knowledge, in `concerns` or the topic it belongs \
         to, then resolve it `promoted`, `merged` or `discarded`: \
         `loom memory resolve <id> --outcome promoted --target <file>#<heading>` (`merged` \
         takes `--target` too; `discarded` takes `--reason <why>`).\n",
    );
}

/// Every `suggestion` entry of the plan's memory journals without a receipt,
/// with the stage whose journal holds it. Reads every journal once and settles
/// entries by the receipts of all of them, as `loom memory pending` does.
fn pending_suggestions(work_dir: &Path) -> Result<Vec<(String, MemoryEntry)>> {
    let (mut entries, _) = read_staged_entries(work_dir, list_journals(work_dir)?, read_journal)?;
    let settled = settled_ids(entries.iter().map(|(_, entry)| entry));
    entries.retain(|(_, entry)| {
        entry.entry_type == MemoryEntryType::Suggestion && !settled.contains(&entry.id)
    });
    Ok(entries)
}
