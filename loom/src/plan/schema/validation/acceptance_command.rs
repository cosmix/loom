//! Acceptance criterion command validation.

use std::ops::RangeInclusive;

use super::super::types::{AcceptanceCriterion, StageDefinition, ValidationError};

/// Maximum length, in characters, of a single acceptance criterion command.
///
/// A real amendment needed 1933 characters to state a corrected criterion
/// precisely; 1024 was too tight to leave room for that.
const MAX_ACCEPTANCE_COMMAND_CHARS: usize = 4096;

/// Accepted `timeout_secs` of an extended criterion or a truth check: zero
/// fails every run, and an hour bounds how long one check can hold a stage.
const TIMEOUT_SECS: RangeInclusive<u64> = 1..=3600;

/// Reject a `timeout_secs` outside [`TIMEOUT_SECS`].
pub(super) fn check_timeout_secs(timeout_secs: Option<u64>) -> Result<(), String> {
    match timeout_secs {
        Some(secs) if !TIMEOUT_SECS.contains(&secs) => Err(format!(
            "timeout_secs {secs} is out of range ({}..={})",
            TIMEOUT_SECS.start(),
            TIMEOUT_SECS.end()
        )),
        _ => Ok(()),
    }
}

/// Validate a single acceptance criterion
///
/// Acceptance criteria must:
/// - Not have an empty or whitespace-only command
/// - Not contain control characters (except whitespace)
/// - Have a reasonable command length (max `MAX_ACCEPTANCE_COMMAND_CHARS` chars)
/// - Keep an extended criterion's `timeout_secs` within [`TIMEOUT_SECS`]
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

    match criterion {
        AcceptanceCriterion::Extended(check) => check_timeout_secs(check.timeout_secs),
        AcceptanceCriterion::Simple(_) => Ok(()),
    }
}

/// Push a `ValidationError` for every acceptance criterion of `stage` that
/// fails `validate_acceptance_criterion`, and for every `before_stage` or
/// `after_stage` truth check whose `timeout_secs` is out of range.
pub(super) fn push_acceptance_errors(stage: &StageDefinition, errors: &mut Vec<ValidationError>) {
    for (idx, criterion) in stage.acceptance.iter().enumerate() {
        if let Err(e) = validate_acceptance_criterion(criterion) {
            errors.push(ValidationError {
                message: format!("Invalid acceptance criterion #{}: {e}", idx + 1),
                stage_id: Some(stage.id.clone()),
            });
        }
    }
    let truth_checks = [
        ("before_stage", &stage.before_stage),
        ("after_stage", &stage.after_stage),
    ];
    for (field, checks) in truth_checks {
        for (idx, check) in checks.iter().enumerate() {
            if let Err(message) = check_timeout_secs(check.timeout_secs) {
                errors.push(ValidationError {
                    message: format!("{field} check #{}: {message}", idx + 1),
                    stage_id: Some(stage.id.clone()),
                });
            }
        }
    }
}
