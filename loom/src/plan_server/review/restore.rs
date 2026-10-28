//! Restore decision for one file of a turn.

use crate::plan_server::wire::paths::to_doc_rel;
use crate::planning::paths::check_relpath_syntax;

/// The text before a turn and the hash after it, when the file exists at each end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinnedFile {
    /// `None` when the turn created the file.
    pub start_text: Option<String>,
    /// `None` when the turn deleted the file.
    pub end_hash: Option<String>,
}

/// The save boundary's result for the pinned text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveResult {
    Saved,
    HashMismatch {
        current_text: String,
        current_hash: String,
    },
    Invalid {
        errors: Vec<String>,
    },
    Refused(String),
}

/// A restore outcome, including the texts needed to resolve a concurrent edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreResult {
    Restored,
    Refused(String),
    Changed {
        current_text: String,
        current_hash: String,
        turn_start_text: String,
    },
    Invalid {
        errors: Vec<String>,
    },
}

/// Restore a doc-relative path through the caller's save boundary.
/// Refusals never call `save`; `force` is forwarded only for a complete, idle pin.
pub fn restore_file(
    path: &str,
    pin: Option<&PinnedFile>,
    turn_running: bool,
    force: bool,
    save: &mut dyn FnMut(&str, &str, &str, bool) -> SaveResult,
) -> RestoreResult {
    if turn_running {
        return RestoreResult::Refused("a turn is running".to_owned());
    }
    let texts = pin.and_then(|pin| pin.start_text.as_deref().zip(pin.end_hash.as_deref()));
    let Some((start_text, end_hash)) = texts else {
        return RestoreResult::Refused(
            "the turn created or deleted this file; ask in chat".to_owned(),
        );
    };
    match save(path, start_text, end_hash, force) {
        SaveResult::Saved => RestoreResult::Restored,
        SaveResult::HashMismatch {
            current_text,
            current_hash,
        } => RestoreResult::Changed {
            current_text,
            current_hash,
            turn_start_text: start_text.to_owned(),
        },
        SaveResult::Invalid { errors } => RestoreResult::Invalid { errors },
        SaveResult::Refused(reason) => RestoreResult::Refused(reason),
    }
}

/// Convert a changed-file path to a syntactically valid doc-relative save path.
pub fn save_path_of(changed_path: &str) -> Option<&str> {
    let path = to_doc_rel(changed_path)?;
    check_relpath_syntax(path).ok()?;
    Some(path)
}
