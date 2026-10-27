//! Plan-input checks for `loom plan verify`: the problems `loom run`'s
//! preflight would refuse on, found with the same discovery.

use std::path::Path;

use super::report::VerifyError;
use crate::plan::inputs::{collect_inputs_with_text, uncommitted_reason, Inputs};

pub(super) fn error(message: String) -> VerifyError {
    VerifyError {
        stage_id: None,
        message,
    }
}

/// Hard errors for invalid and missing inputs, plus one note listing inputs
/// that are present but not committed. Committed-ness is repository state,
/// not plan correctness, so it never becomes an error or warning. `plan_text`, when given, is
/// the plan's content in place of the file on disk.
pub(super) fn input_findings(
    root: &Path,
    plan: &Path,
    plan_text: Option<&str>,
) -> (Vec<VerifyError>, Vec<String>) {
    let (Ok(root), Some(parent), Some(name)) =
        (root.canonicalize(), plan.parent(), plan.file_name())
    else {
        return (Vec::new(), Vec::new());
    };
    // Discovery strips the canonical root from the plan path.
    let plan = parent
        .canonicalize()
        .map_or_else(|_| plan.to_path_buf(), |p| p.join(name));
    let inputs = match collect_inputs_with_text(&root, &plan, plan_text) {
        Ok(inputs) => inputs,
        Err(e) => {
            return (
                vec![error(format!("Cannot check plan inputs: {e:#}"))],
                Vec::new(),
            )
        }
    };
    let mut errors: Vec<VerifyError> = inputs
        .invalid
        .iter()
        .map(|reference| error(reference.to_string()))
        .collect();
    let notes = file_findings(&root, &inputs, &mut errors);
    (errors, notes)
}

/// Push an error for each input file that is missing; return the note listing the uncommitted
/// ones, or no note when there are none or git cannot tell.
fn file_findings(root: &Path, inputs: &Inputs, errors: &mut Vec<VerifyError>) -> Vec<String> {
    let mut uncommitted = Vec::new();
    let mut git_available = true;
    for (path, origin) in &inputs.files {
        match uncommitted_reason(root, path) {
            Ok(Some("missing file")) => errors.push(error(format!(
                "{} (referenced in {origin}): missing file; loom run refuses to start without it",
                path.display()
            ))),
            Ok(Some(reason)) => uncommitted.push(format!("{} ({reason})", path.display())),
            Ok(None) => {}
            Err(_) => git_available = false,
        }
    }
    if git_available && !uncommitted.is_empty() {
        vec![format!(
            "{} plan input(s) not committed; loom run refuses to start until they are \
             committed: {}",
            uncommitted.len(),
            uncommitted.join(", ")
        )]
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests;
