//! A freeze-refusal dispute's verdicts (plan Decisions 8-12), routed here by
//! `apply_kinds.rs`. The refused contract writer is retired before any of
//! them applies (`core/verdict_apply.rs`). Each one re-queues the stage, held
//! while a sibling dispute is unanswered, for a fresh writer on the same
//! worktree: no freeze exists, so the stage starts with its contract phase
//! again, and its signal carries the notice written here. No freeze record
//! is read or written, and no verdict sends the stage to a human except the
//! amendment cap.
//!
//! * accept: the plan was wrong. Each `plan_patch.patches` entry is one
//!   amendment, applied in array order with the dispute id `<dispute>.<k>`,
//!   so a retried apply skips what an earlier attempt applied. At the
//!   amendment cap `amend_plan` escalates instead and nothing more applies.
//! * reject: the writer's work must change; the stored reasoning carries the
//!   adjudicator's instructions.
//! * needs-more-evidence: the writer answers in `loom memory note` and
//!   freezes again. A refusal of that freeze is a new filing, which resets the
//!   evidence rounds, so the freeze-refusal budget bounds the exchange.

use anyhow::Result;
use std::collections::BTreeSet;
use std::path::Path;

use crate::context::untrusted::inline_safe;
use crate::models::dispute::{strip_control, PlanPatch};
use crate::models::stage::Stage;
use crate::plan::amendment::{AmendmentField, AmendmentPatch, AmendmentRequest};
use crate::plan::schema::ContractSpec;
use crate::verify::contracts::changes::stage_base;
use crate::verify::contracts::collection::stage_scaffold;
use crate::verify::contracts::normalize;
use crate::verify::review::report::single_line;

use super::apply::amend_plan;
use super::apply_kinds::notify_and_requeue;
use super::prompt::scaffold_line;
use super::verdict_freeze_refusal::{decode_patches, FreezePatch};

/// Amend the plan by every patch of an accepted verdict, then re-queue the
/// stage with the notice of what changed.
pub(super) fn accept(
    work_dir: &Path,
    stage: &mut Stage,
    dispute: u32,
    plan_patch: &PlanPatch,
    reasoning: &str,
) -> Result<()> {
    let patches = decode_patches(&plan_patch.inner).map_err(|msg| anyhow::anyhow!(msg))?;
    // On a retry after a partial apply the stage already holds some of the
    // amendments, so the notice may then miss an old contract file.
    let before = stage.contracts.clone();
    for (k, entry) in patches.iter().enumerate() {
        let request = AmendmentRequest {
            stage_id: stage.id.clone(),
            field: entry.field,
            patch: entry.patch.clone(),
            reason: Some(
                entry
                    .reason
                    .clone()
                    .unwrap_or_else(|| format!("freeze-refusal dispute {dispute} accepted")),
            ),
            dispute_id: Some(format!("{dispute}.{k}")),
        };
        if !amend_plan(work_dir, stage, request)? {
            return Ok(());
        }
    }
    let notice = accept_notice(work_dir, stage, dispute, &patches, &before, reasoning);
    notify_and_requeue(work_dir, stage, &notice)
}

/// The feedback for a rejected freeze refusal. `reasoning` is the stored one,
/// its `### Instructions` beneath it.
pub(super) fn reject_notice(stage_id: &str, reasoning: &str) -> String {
    format!(
        "The adjudicator rejected the refused contract freeze: the refusal stands and the \
         contract work must change.\n\n### Reasoning\n\n{}\n\nAction: change what the \
         instructions name, then run `loom stage contracts freeze {stage_id}` and stop.\n",
        strip_control(reasoning, &['\n', '\t']).trim()
    )
}

/// Ask the next writer the adjudicator's questions; it answers in memory
/// notes, where the next adjudicator reads them.
pub(super) fn needs_more_evidence(
    work_dir: &Path,
    stage: &mut Stage,
    questions: &[String],
) -> Result<()> {
    let mut body = format!(
        "The adjudicator needs more evidence. Answer each question with `loom memory note \
         \"freeze-dispute: answer <n>: <evidence>\"`, then run `loom stage contracts freeze \
         {}` and stop; a refused freeze reaches the adjudicator again.\n\n",
        stage.id
    );
    for (n, question) in questions.iter().enumerate() {
        body.push_str(&format!("{}. {}\n", n + 1, single_line(question)));
    }
    stage.tally.evidence_rounds = stage.tally.evidence_rounds.saturating_add(1);
    notify_and_requeue(work_dir, stage, &body)
}

fn accept_notice(
    work_dir: &Path,
    stage: &Stage,
    dispute: u32,
    patches: &[FreezePatch],
    before: &[ContractSpec],
    reasoning: &str,
) -> String {
    let mut body = format!(
        "The adjudicator accepted the refused contract freeze (dispute {dispute}): the plan \
         was wrong and is amended.\n\n### Amendments\n\n"
    );
    for entry in patches {
        body.push_str(&format!("- {}\n", amendment_line(entry)));
    }
    body.push_str(&format!(
        "\n### Reasoning\n\n{}\n\n",
        strip_control(reasoning, &['\n', '\t']).trim()
    ));
    body.push_str(&old_files_section(work_dir, stage, before));
    body.push_str(&format!(
        "Action: write the contracts as this signal's Contracts, Harness and Scaffold sections \
         now state them, then run `loom stage contracts freeze {}` and stop.\n",
        stage.id
    ));
    body
}

/// `harness: inserted at 0: tests/fixtures/**`.
fn amendment_line(entry: &FreezePatch) -> String {
    let field = match entry.field {
        AmendmentField::Contracts => "contracts",
        AmendmentField::Harness => "harness",
        AmendmentField::Acceptance => "acceptance",
        AmendmentField::Wiring => "wiring",
        AmendmentField::WiringTests => "wiring-tests",
    };
    let (verb, value) = match &entry.patch {
        AmendmentPatch::Replace { value, .. } => ("replaced", Some(value)),
        AmendmentPatch::Insert { value, .. } => ("inserted at", Some(value)),
        AmendmentPatch::Delete { .. } => ("deleted", None),
    };
    let index = entry.patch.index();
    match value.map(|value| shown_value(entry.field, value)) {
        Some(shown) => format!("{field}: {verb} {index}: {shown}"),
        None => format!("{field}: {verb} {index}"),
    }
}

/// A contract by its id, any other value on one line. Both are the judge's
/// text, flattened so no backtick or line break escapes the code span.
fn shown_value(field: AmendmentField, value: &str) -> String {
    if field == AmendmentField::Contracts {
        return crate::yaml::from_str::<ContractSpec>(value)
            .map(|spec| format!("contract `{}`", inline_safe(&spec.id)))
            .unwrap_or_else(|_| "a contract".to_string());
    }
    format!("`{}`", inline_safe(value.trim()))
}

/// The contract files the amendment left behind in the reused worktree, each
/// with the scaffold it alone needed, for the next writer to delete. Empty
/// when every contract file still stands.
fn old_files_section(work_dir: &Path, stage: &Stage, before: &[ContractSpec]) -> String {
    let current: BTreeSet<String> = stage.contracts.iter().map(|c| normalize(&c.file)).collect();
    let removed: Vec<ContractSpec> = before
        .iter()
        .filter(|contract| !current.contains(&normalize(&contract.file)))
        .cloned()
        .collect();
    if removed.is_empty() {
        return String::new();
    }
    let mut section = String::from(
        "### Old contract files\n\nThe worktree is reused. Delete each file below, which no \
         contract names any more, and the scaffold it needed unless this signal's Scaffold \
         section still lists it:\n\n",
    );
    for contract in &removed {
        section.push_str(&format!(
            "- `{}`\n",
            single_line(&normalize(&contract.file))
        ));
    }
    match old_scaffold(work_dir, stage, &removed) {
        Ok(lines) if lines.is_empty() => {}
        Ok(lines) => {
            section.push_str("\nTheir scaffold:\n\n");
            for line in lines {
                section.push_str(&format!("- {line}\n"));
            }
        }
        Err(error) => section.push_str(&format!("\n(their scaffold is unknown: {error})\n")),
    }
    section.push('\n');
    section
}

/// The scaffold `removed` needed, derived over the stage base.
fn old_scaffold(
    work_dir: &Path,
    stage: &Stage,
    removed: &[ContractSpec],
) -> Result<Vec<String>, String> {
    let site = super::locate_stage_site(work_dir, stage).map_err(|e| format!("{e:#}"))?;
    let base = stage_base(&site.git, work_dir).map_err(|e| format!("{e:#}"))?;
    let (git, root, dir) = (&site.git, &site.worktree_root, &site.working_dir);
    let edits = stage_scaffold(git, root, dir, &base, removed)?;
    Ok(edits.iter().map(scaffold_line).collect())
}

#[cfg(test)]
#[path = "apply_freeze_refusal_tests.rs"]
mod tests;
