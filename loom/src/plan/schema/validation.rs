//! Plan YAML schema validation

use crate::validation::validate_id;

use super::types::{LoomMetadata, ValidationError};

/// Validate a single acceptance criterion
///
/// Acceptance criteria must:
/// - Not be empty or whitespace-only
/// - Not contain control characters (except whitespace)
/// - Have a reasonable length (max 1024 chars)
pub(crate) fn validate_acceptance_criterion(criterion: &str) -> Result<(), String> {
    // Check for empty or whitespace-only
    let trimmed = criterion.trim();
    if trimmed.is_empty() {
        return Err("acceptance criterion cannot be empty".to_string());
    }

    // Check length limit
    if criterion.len() > 1024 {
        return Err(format!(
            "acceptance criterion too long ({} chars, max 1024)",
            criterion.len()
        ));
    }

    // Check for control characters (except tab, newline, carriage return)
    for (idx, ch) in criterion.chars().enumerate() {
        if ch.is_control() && ch != '\t' && ch != '\n' && ch != '\r' {
            return Err(format!(
                "acceptance criterion contains control character at position {idx}"
            ));
        }
    }

    Ok(())
}

/// Validate the loom metadata
pub fn validate(metadata: &LoomMetadata) -> Result<(), Vec<ValidationError>> {
    let mut errors = Vec::new();

    // Check version
    if metadata.loom.version != 1 {
        errors.push(ValidationError {
            message: format!(
                "Unsupported version: {}. Only version 1 is supported.",
                metadata.loom.version
            ),
            stage_id: None,
        });
    }

    // Check for empty stages
    if metadata.loom.stages.is_empty() {
        errors.push(ValidationError {
            message: "No stages defined".to_string(),
            stage_id: None,
        });
    }

    // Collect all stage IDs
    let stage_ids: std::collections::HashSet<_> =
        metadata.loom.stages.iter().map(|s| &s.id).collect();

    // Validate each stage
    for stage in &metadata.loom.stages {
        // Check for empty ID
        if stage.id.is_empty() {
            errors.push(ValidationError {
                message: "Stage ID cannot be empty".to_string(),
                stage_id: None,
            });
            continue;
        }

        // Validate stage ID is safe for file paths (prevents path traversal attacks)
        if let Err(e) = validate_id(&stage.id) {
            errors.push(ValidationError {
                message: format!("Invalid stage ID: {e}"),
                stage_id: Some(stage.id.clone()),
            });
        }

        // Check for empty name
        if stage.name.is_empty() {
            errors.push(ValidationError {
                message: "Stage name cannot be empty".to_string(),
                stage_id: Some(stage.id.clone()),
            });
        }

        // Validate dependencies exist and have valid IDs
        for dep in &stage.dependencies {
            // Validate dependency ID format (prevents path traversal in dependency refs)
            if let Err(e) = validate_id(dep) {
                errors.push(ValidationError {
                    message: format!("Invalid dependency ID '{dep}': {e}"),
                    stage_id: Some(stage.id.clone()),
                });
                continue;
            }

            if !stage_ids.contains(dep) {
                errors.push(ValidationError {
                    message: format!("Unknown dependency: '{dep}'"),
                    stage_id: Some(stage.id.clone()),
                });
            }

            // Check for self-dependency
            if dep == &stage.id {
                errors.push(ValidationError {
                    message: "Stage cannot depend on itself".to_string(),
                    stage_id: Some(stage.id.clone()),
                });
            }
        }

        // Validate acceptance criteria
        for (idx, criterion) in stage.acceptance.iter().enumerate() {
            if let Err(e) = validate_acceptance_criterion(criterion) {
                errors.push(ValidationError {
                    message: format!("Invalid acceptance criterion #{}: {e}", idx + 1),
                    stage_id: Some(stage.id.clone()),
                });
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
