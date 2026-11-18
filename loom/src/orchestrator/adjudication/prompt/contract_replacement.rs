//! What a contract dispute's briefing adds when the dispute carries a
//! replacement for the frozen file. The judge writes no files and runs no
//! replacement: it reads one against the contract's scenario and rejects
//! clause, and completion runs the accepted file before the stage can finish.

use super::super::diff::unified_diff;
use super::super::{fenced, KindPromptInput};
use crate::models::dispute::DisputeKind;
use crate::plan::schema::ContractSpec;

/// The evidence headings whose fences the truncation pass halves
/// (`truncate.rs`), next to the other contract-file sections.
pub(crate) const REPLACEMENT_HEADING: &str = "## Proposed replacement";
pub(crate) const REPLACEMENT_DIFF_HEADING: &str = "## Diff (frozen vs replacement)";

/// Said in every contract prompt: no loom hook stops an edit of a frozen file,
/// and the evidence the judge asks for must be something the agent can
/// produce.
pub(super) const NEVER_EDIT_SENTENCE: &str = "Never ask the agent to edit a frozen file; ask only for evidence it can produce read-only, or for a revised `--replacement`.\n\n";

/// The replacement text the dispute carries.
pub(super) fn proposed<'a>(input: &'a KindPromptInput<'_>) -> Option<&'a str> {
    match &input.request.kind {
        DisputeKind::Contract { replacement, .. } => replacement.as_deref(),
        _ => None,
    }
}

/// Appended to the first step: the command runs the frozen file, not the
/// replacement, so its result is no evidence for the replacement.
pub(super) fn first_step_note(has_replacement: bool) -> &'static str {
    if has_replacement {
        "The command above runs the frozen content; the replacement has not been run. Judge the replacement by reading it against the scenario and rejects clause. Completion runs it after an accept.\n\n"
    } else {
        ""
    }
}

/// Appended to the accept bullet of the verdict semantics.
pub(super) fn accept_note(has_replacement: bool) -> &'static str {
    if has_replacement {
        "  With a proposed replacement, accept means the replacement still covers the\n  scenario and rejects the stated plausible wrong implementation. An accept\n  writes the replacement into the worktree, then re-freezes.\n"
    } else {
        ""
    }
}

/// What the citation sentence lists besides the contract and the diff.
pub(super) fn citable(has_replacement: bool) -> &'static str {
    if has_replacement {
        "proposed replacement, "
    } else {
        ""
    }
}

/// The `## Proposed replacement` section and the frozen -> replacement diff;
/// empty without a replacement.
pub(super) fn sections(
    input: &KindPromptInput<'_>,
    contract: Option<&ContractSpec>,
    frozen: &Result<String, String>,
) -> String {
    let Some(text) = proposed(input) else {
        return String::new();
    };
    let mut s = format!("{REPLACEMENT_HEADING}\n\n");
    s.push_str(&fenced("text", text));
    s.push_str(&format!("{REPLACEMENT_DIFF_HEADING}\n\n"));
    match (frozen, contract) {
        (Ok(before), Some(contract)) => {
            s.push_str(&fenced("diff", &unified_diff(before, text, &contract.file)))
        }
        _ => s.push_str("(diff unavailable because the frozen content is unavailable)\n\n"),
    }
    s
}
