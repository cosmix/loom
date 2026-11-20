//! What a contract dispute's briefing adds when the dispute carries
//! replacements for frozen files. The judge writes no files and runs no
//! replacement: it reads one against the scenario and rejects clause of the
//! contracts in its file, and completion runs the accepted file before the
//! stage can finish.

use super::super::diff::unified_diff;
use super::super::{fenced, KindPromptInput};
use crate::models::dispute::{DisputeKind, FileReplacement};
use crate::plan::schema::ContractSpec;
use crate::utils::sha256_hex;
use crate::verify::contracts::normalize;
use crate::verify::contracts::trial::{load_receipt, receipt_covers};
use crate::verify::integrity::lost_statements;

/// The evidence headings whose fences the truncation pass halves
/// (`truncate.rs`), next to the other contract-file sections. Each replaced
/// file has a section of each; a line under the heading names the file.
pub(crate) const REPLACEMENT_HEADING: &str = "## Proposed replacement";
pub(crate) const REPLACEMENT_DIFF_HEADING: &str = "## Diff (frozen vs replacement)";
/// Under each file's diff: the assertions the replacement drops or changes.
const LOST_HEADING: &str = "### Assertions the replacement drops or changes";

/// Said in every contract prompt: no loom hook stops an edit of a frozen file,
/// and the evidence the judge asks for must be something the agent can
/// produce.
pub(super) const NEVER_EDIT_SENTENCE: &str = "Never ask the agent to edit a frozen file; ask only for evidence it can produce read-only, or for a revised `--replacement`.\n\n";

/// The replacements the dispute carries, one per frozen file.
pub(super) fn proposed<'a>(input: &'a KindPromptInput<'_>) -> &'a [FileReplacement] {
    match &input.request.kind {
        DisputeKind::Contract { replacements, .. } => replacements,
        _ => &[],
    }
}

/// Appended to the first step: the commands run the frozen files, not the
/// replacements, so their results are no evidence for a replacement.
pub(super) fn first_step_note(has_replacement: bool) -> &'static str {
    if has_replacement {
        "The commands above run the frozen content, not a replacement; its trial result is the `Trial:` line under `## Proposed replacement`. Judge each replacement by reading it against the scenario and rejects clause of the contracts in its file. Completion runs it after an accept.\n\n"
    } else {
        ""
    }
}

/// The ids of the contracts whose file is `file`.
pub(super) fn contracts_in<'a>(
    file: &str,
    specs: &[(&'a str, Option<&ContractSpec>)],
) -> Vec<&'a str> {
    let file = normalize(file);
    specs
        .iter()
        .filter(|(_, spec)| spec.is_some_and(|spec| normalize(&spec.file) == file))
        .map(|(id, _)| *id)
        .collect()
}

/// The trial line of each `## Proposed replacement` section: admission files
/// replacements only once a trial of the same bytes passed at the worktree's
/// head, so the newest receipt names the commit it ran at when it covers every
/// `(file, sha256)` pair of this filing. A receipt that was pruned, cannot be
/// read or ran other bytes is said to be missing.
fn trial_line(input: &KindPromptInput<'_>) -> String {
    let pairs: Vec<(String, String)> = proposed(input)
        .iter()
        .map(|r| (r.file.clone(), sha256_hex(r.content.as_bytes())))
        .collect();
    match load_receipt(input.work_dir, &input.stage.id) {
        Ok(Some(receipt)) if receipt_covers(Some(&receipt), &receipt.head, &pairs).is_ok() => {
            format!(
                "Trial: passed in a confined host job at {} (format and static gates, then the \
                 contracts)\n\n",
                receipt.head
            )
        }
        _ => "Trial: no matching trial receipt is still recorded\n\n".to_string(),
    }
}

/// The assertions of the frozen file the replacement does not keep, one per
/// line, or `none`.
fn lost_section(file: &str, frozen: &str, replacement: &str) -> String {
    let lost = lost_statements(file, frozen, replacement);
    let mut s = format!("{LOST_HEADING}\n\nFile: `{file}`\n\n");
    if lost.is_empty() {
        s.push_str("none\n\n");
    } else {
        s.push_str(&fenced("text", &lost.join("\n")));
    }
    s
}

/// Per replaced file, a `## Proposed replacement` section, a frozen ->
/// replacement diff and the assertions the replacement drops, each naming the
/// file and the disputed contracts that live in it; empty without
/// replacements. `frozen` reads a frozen file's content.
pub(super) fn sections(
    input: &KindPromptInput<'_>,
    specs: &[(&str, Option<&ContractSpec>)],
    frozen: impl Fn(&str) -> Result<String, String>,
) -> String {
    let mut s = String::new();
    let trial = if proposed(input).is_empty() {
        String::new()
    } else {
        trial_line(input)
    };
    for replacement in proposed(input) {
        let file = &replacement.file;
        let ids = contracts_in(file, specs);
        let owners = if ids.is_empty() {
            "no disputed contract (a harness or support file)".to_string()
        } else {
            format!("contracts: {}", ids.join(", "))
        };
        s.push_str(&format!(
            "{REPLACEMENT_HEADING}\n\nFile: `{file}` ({owners})\n\n{trial}"
        ));
        s.push_str(&fenced("text", &replacement.content));
        s.push_str(&format!("{REPLACEMENT_DIFF_HEADING}\n\nFile: `{file}`\n\n"));
        match frozen(file) {
            Ok(before) => {
                s.push_str(&fenced(
                    "diff",
                    &unified_diff(&before, &replacement.content, file),
                ));
                s.push_str(&lost_section(file, &before, &replacement.content));
            }
            Err(_) => {
                s.push_str("(diff unavailable because the frozen content is unavailable)\n\n")
            }
        }
    }
    s
}
