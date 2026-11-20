//! What a contract dispute's briefing adds when the dispute carries
//! replacements for frozen files. The judge writes no files and runs no
//! replacement: it reads one against the scenario and rejects clause of the
//! contracts in its file, and completion runs the accepted file before the
//! stage can finish.

use super::super::diff::unified_diff;
use super::super::{fenced, KindPromptInput};
use crate::models::dispute::{DisputeKind, FileReplacement};
use crate::plan::schema::ContractSpec;
use crate::verify::contracts::normalize;

/// The evidence headings whose fences the truncation pass halves
/// (`truncate.rs`), next to the other contract-file sections. Each replaced
/// file has a section of each; a line under the heading names the file.
pub(crate) const REPLACEMENT_HEADING: &str = "## Proposed replacement";
pub(crate) const REPLACEMENT_DIFF_HEADING: &str = "## Diff (frozen vs replacement)";

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
        "The commands above run the frozen content; a replacement has not been run. Judge each replacement by reading it against the scenario and rejects clause of the contracts in its file. Completion runs it after an accept.\n\n"
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

/// Per replaced file, a `## Proposed replacement` section and a frozen ->
/// replacement diff, each naming the file and the disputed contracts that live
/// in it; empty without replacements. `frozen` reads a frozen file's content.
pub(super) fn sections(
    input: &KindPromptInput<'_>,
    specs: &[(&str, Option<&ContractSpec>)],
    frozen: impl Fn(&str) -> Result<String, String>,
) -> String {
    let mut s = String::new();
    for replacement in proposed(input) {
        let file = &replacement.file;
        let ids = contracts_in(file, specs);
        let owners = if ids.is_empty() {
            "no disputed contract (a harness or support file)".to_string()
        } else {
            format!("contracts: {}", ids.join(", "))
        };
        s.push_str(&format!(
            "{REPLACEMENT_HEADING}\n\nFile: `{file}` ({owners})\n\n"
        ));
        s.push_str(&fenced("text", &replacement.content));
        s.push_str(&format!("{REPLACEMENT_DIFF_HEADING}\n\nFile: `{file}`\n\n"));
        match frozen(file) {
            Ok(before) => s.push_str(&fenced(
                "diff",
                &unified_diff(&before, &replacement.content, file),
            )),
            Err(_) => {
                s.push_str("(diff unavailable because the frozen content is unavailable)\n\n")
            }
        }
    }
    s
}
