//! Acceptance criterion command validation.

use super::super::types::AcceptanceCriterion;

/// Maximum length, in characters, of a single acceptance criterion command.
///
/// A real amendment needed 1933 characters to state a corrected criterion
/// precisely; 1024 was too tight to leave room for that.
const MAX_ACCEPTANCE_COMMAND_CHARS: usize = 4096;

/// Validate a single acceptance criterion
///
/// Acceptance criteria must:
/// - Not have an empty or whitespace-only command
/// - Not contain control characters (except whitespace)
/// - Have a reasonable command length (max `MAX_ACCEPTANCE_COMMAND_CHARS` chars)
pub(crate) fn validate_acceptance_criterion(criterion: &AcceptanceCriterion) -> Result<(), String> {
    let command = criterion.command();

    // Check for empty or whitespace-only
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Err("acceptance criterion command cannot be empty".to_string());
    }

    // Check length limit
    if command.len() > MAX_ACCEPTANCE_COMMAND_CHARS {
        return Err(format!(
            "acceptance criterion command too long ({} chars, max {MAX_ACCEPTANCE_COMMAND_CHARS})",
            command.len()
        ));
    }

    // Check for control characters (except tab, newline, carriage return)
    for (idx, ch) in command.chars().enumerate() {
        if ch.is_control() && ch != '\t' && ch != '\n' && ch != '\r' {
            return Err(format!(
                "acceptance criterion contains control character at position {idx}"
            ));
        }
    }

    Ok(())
}
