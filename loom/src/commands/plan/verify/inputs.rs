//! Plan-input checks for `loom plan verify`: the problems `loom run`'s
//! preflight would refuse on, found with the same discovery.

use std::path::Path;

use super::JsonError;
use crate::plan::inputs::{collect_inputs, uncommitted_reason};

fn error(message: String) -> JsonError {
    JsonError {
        stage_id: None,
        message,
    }
}

/// Hard errors for invalid and missing inputs, plus one note listing inputs
/// that are present but not committed. Committed-ness is repository state,
/// not plan correctness, so it never becomes an error or warning.
pub(super) fn input_findings(root: &Path, plan: &Path) -> (Vec<JsonError>, Vec<String>) {
    let (Ok(root), Some(parent), Some(name)) =
        (root.canonicalize(), plan.parent(), plan.file_name())
    else {
        return (Vec::new(), Vec::new());
    };
    // Discovery strips the canonical root from the plan path.
    let plan = parent
        .canonicalize()
        .map_or_else(|_| plan.to_path_buf(), |p| p.join(name));
    let inputs = match collect_inputs(&root, &plan) {
        Ok(inputs) => inputs,
        Err(e) => {
            return (
                vec![error(format!("Cannot check plan inputs: {e:#}"))],
                Vec::new(),
            )
        }
    };
    let mut errors: Vec<JsonError> = inputs
        .invalid
        .iter()
        .map(|reference| error(reference.to_string()))
        .collect();
    let mut uncommitted = Vec::new();
    let mut git_available = true;
    for (path, origin) in &inputs.files {
        match uncommitted_reason(&root, path) {
            Ok(Some("missing file")) => errors.push(error(format!(
                "{} (referenced in {origin}): missing file; loom run refuses to start without it",
                path.display()
            ))),
            Ok(Some(reason)) => uncommitted.push(format!("{} ({reason})", path.display())),
            Ok(None) => {}
            Err(_) => git_available = false,
        }
    }
    let notes = if git_available && !uncommitted.is_empty() {
        vec![format!(
            "{} plan input(s) not committed; loom run refuses to start until they are \
             committed: {}",
            uncommitted.len(),
            uncommitted.join(", ")
        )]
    } else {
        Vec::new()
    };
    (errors, notes)
}

#[cfg(test)]
mod tests;
