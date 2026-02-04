//! Plan YAML schema validation

use crate::validation::validate_id;

use super::types::{
    FilesystemConfig, LoomMetadata, NetworkConfig, SandboxConfig, StageSandboxConfig,
    ValidationError,
};

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

/// Validate a glob pattern is syntactically correct
fn validate_glob_pattern(pattern: &str) -> Result<(), String> {
    // Use the glob crate's Pattern::new() to validate
    glob::Pattern::new(pattern)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Validate a domain pattern
fn validate_domain_pattern(domain: &str) -> Result<(), String> {
    // Check for basic validity:
    // - Not empty
    // - No spaces
    // - Valid characters (alphanumeric, dots, dashes, wildcards)
    if domain.is_empty() {
        return Err("domain cannot be empty".to_string());
    }
    if domain.contains(' ') {
        return Err("domain cannot contain spaces".to_string());
    }
    // Allow wildcards like *.example.com
    let clean = domain.replace('*', "x");
    // Simple validation - could be more strict
    if clean
        .chars()
        .all(|c| c.is_alphanumeric() || c == '.' || c == '-')
    {
        Ok(())
    } else {
        Err("invalid characters in domain".to_string())
    }
}

/// Validate filesystem configuration
fn validate_filesystem_config(
    filesystem: &FilesystemConfig,
    errors: &mut Vec<ValidationError>,
    stage_id: Option<&str>,
) {
    // Validate deny_read paths are valid globs
    for path in &filesystem.deny_read {
        if let Err(e) = validate_glob_pattern(path) {
            errors.push(ValidationError {
                message: format!("Invalid sandbox deny_read glob pattern '{}': {}", path, e),
                stage_id: stage_id.map(|s| s.to_string()),
            });
        }
    }

    // Validate deny_write paths are valid globs
    for path in &filesystem.deny_write {
        if let Err(e) = validate_glob_pattern(path) {
            errors.push(ValidationError {
                message: format!("Invalid sandbox deny_write glob pattern '{}': {}", path, e),
                stage_id: stage_id.map(|s| s.to_string()),
            });
        }
    }

    // Validate allow_write paths are valid globs
    for path in &filesystem.allow_write {
        if let Err(e) = validate_glob_pattern(path) {
            errors.push(ValidationError {
                message: format!("Invalid sandbox allow_write glob pattern '{}': {}", path, e),
                stage_id: stage_id.map(|s| s.to_string()),
            });
        }
    }
}

/// Validate network configuration
fn validate_network_config(
    network: &NetworkConfig,
    errors: &mut Vec<ValidationError>,
    stage_id: Option<&str>,
) {
    // Validate allowed_domains
    for domain in &network.allowed_domains {
        if let Err(e) = validate_domain_pattern(domain) {
            errors.push(ValidationError {
                message: format!("Invalid sandbox allowed_domain '{}': {}", domain, e),
                stage_id: stage_id.map(|s| s.to_string()),
            });
        }
    }

    // Validate additional_domains
    for domain in &network.additional_domains {
        if let Err(e) = validate_domain_pattern(domain) {
            errors.push(ValidationError {
                message: format!("Invalid sandbox additional_domain '{}': {}", domain, e),
                stage_id: stage_id.map(|s| s.to_string()),
            });
        }
    }
}

/// Validate sandbox configuration at plan level
fn validate_sandbox_config(sandbox: &SandboxConfig, errors: &mut Vec<ValidationError>) {
    // Validate filesystem paths are valid globs
    validate_filesystem_config(&sandbox.filesystem, errors, None);

    // Validate domain patterns
    validate_network_config(&sandbox.network, errors, None);
}

/// Validate sandbox configuration at stage level
fn validate_stage_sandbox_config(
    sandbox: &StageSandboxConfig,
    errors: &mut Vec<ValidationError>,
    stage_id: &str,
) {
    // Validate optional filesystem if present
    if let Some(fs) = &sandbox.filesystem {
        validate_filesystem_config(fs, errors, Some(stage_id));
    }

    // Validate optional network if present
    if let Some(net) = &sandbox.network {
        validate_network_config(net, errors, Some(stage_id));
    }
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

    // Validate plan-level sandbox configuration
    validate_sandbox_config(&metadata.loom.sandbox, &mut errors);

    // Check for empty stages
    if metadata.loom.stages.is_empty() {
        errors.push(ValidationError {
            message: "No stages defined".to_string(),
            stage_id: None,
        });
    }

    // Collect all stage IDs and detect duplicates
    let stage_ids: std::collections::HashSet<_> =
        metadata.loom.stages.iter().map(|s| &s.id).collect();

    // Check for duplicate stage IDs
    if stage_ids.len() != metadata.loom.stages.len() {
        errors.push(ValidationError {
            message: "Duplicate stage IDs detected".to_string(),
            stage_id: None,
        });
    }

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

        // Validate working_dir to prevent path traversal
        if stage.working_dir.contains("..") {
            errors.push(ValidationError {
                message: "working_dir cannot contain path traversal (..)".to_string(),
                stage_id: Some(stage.id.clone()),
            });
        }
        if stage.working_dir.starts_with('/') {
            errors.push(ValidationError {
                message: "working_dir must be relative path".to_string(),
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

        // Validate truths
        if stage.truths.len() > 20 {
            errors.push(ValidationError {
                message: format!("Too many truths ({}, max 20)", stage.truths.len()),
                stage_id: Some(stage.id.clone()),
            });
        }
        for (idx, truth) in stage.truths.iter().enumerate() {
            if truth.len() > 500 {
                errors.push(ValidationError {
                    message: format!(
                        "Truth #{} too long ({} chars, max 500)",
                        idx + 1,
                        truth.len()
                    ),
                    stage_id: Some(stage.id.clone()),
                });
            }
            if truth.trim().is_empty() {
                errors.push(ValidationError {
                    message: format!("Truth #{} cannot be empty", idx + 1),
                    stage_id: Some(stage.id.clone()),
                });
            }
        }

        // Validate artifacts
        if stage.artifacts.len() > 100 {
            errors.push(ValidationError {
                message: format!("Too many artifacts ({}, max 100)", stage.artifacts.len()),
                stage_id: Some(stage.id.clone()),
            });
        }
        for (idx, artifact) in stage.artifacts.iter().enumerate() {
            if artifact.contains("..") {
                errors.push(ValidationError {
                    message: format!("Artifact #{} contains path traversal (..)", idx + 1),
                    stage_id: Some(stage.id.clone()),
                });
            }
            if artifact.starts_with('/') {
                errors.push(ValidationError {
                    message: format!("Artifact #{} must be relative path", idx + 1),
                    stage_id: Some(stage.id.clone()),
                });
            }
        }

        // Validate wiring checks
        for (idx, wiring) in stage.wiring.iter().enumerate() {
            // Validate source path
            if wiring.source.contains("..") {
                errors.push(ValidationError {
                    message: format!("Wiring #{} source contains path traversal (..)", idx + 1),
                    stage_id: Some(stage.id.clone()),
                });
            }
            if wiring.source.starts_with('/') {
                errors.push(ValidationError {
                    message: format!("Wiring #{} source must be relative path", idx + 1),
                    stage_id: Some(stage.id.clone()),
                });
            }
            // Validate pattern is valid regex
            if let Err(e) = regex::Regex::new(&wiring.pattern) {
                errors.push(ValidationError {
                    message: format!("Wiring #{} has invalid regex pattern: {}", idx + 1, e),
                    stage_id: Some(stage.id.clone()),
                });
            }
            // Validate description not empty
            if wiring.description.trim().is_empty() {
                errors.push(ValidationError {
                    message: format!("Wiring #{} description cannot be empty", idx + 1),
                    stage_id: Some(stage.id.clone()),
                });
            }
        }

        // Require goal-backward checks for standard stages
        // Knowledge, IntegrationVerify, and CodeReview stages are exempt (they have different purposes)
        if stage.stage_type == super::types::StageType::Standard {
            let has_goal_checks =
                !stage.truths.is_empty() || !stage.artifacts.is_empty() || !stage.wiring.is_empty();

            if !has_goal_checks {
                errors.push(ValidationError {
                    message: "Standard stages must define at least one truth, artifact, or wiring check. \
                             These define observable outcomes that verify the stage actually works."
                        .to_string(),
                    stage_id: Some(stage.id.clone()),
                });
            }
        }

        // Validate stage-level sandbox configuration
        validate_stage_sandbox_config(&stage.sandbox, &mut errors, &stage.id);
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Check for knowledge-related recommendations (non-fatal warnings)
///
/// Returns a list of warning messages when:
/// - Plan has root stages (no dependencies) but lacks a knowledge-bootstrap stage
pub fn check_knowledge_recommendations(stages: &[super::types::StageDefinition]) -> Vec<String> {
    let mut warnings = Vec::new();

    // Check if any stage has "knowledge" in its ID or name (case-insensitive)
    let has_knowledge_stage = stages.iter().any(|stage| {
        stage.id.to_lowercase().contains("knowledge")
            || stage.name.to_lowercase().contains("knowledge")
    });

    // Find root stages (stages with no dependencies)
    let has_root_stages = stages.iter().any(|stage| stage.dependencies.is_empty());

    // Warn if there are root stages but no knowledge stage
    if has_root_stages && !has_knowledge_stage {
        warnings.push(
            "Consider adding a 'knowledge-bootstrap' stage to capture codebase knowledge. \
             This stage can run first (no dependencies) to document entry points, patterns, \
             and conventions for subsequent stages."
                .to_string(),
        );
    }

    warnings
}

/// Check for sandbox-related recommendations (non-fatal warnings)
///
/// Returns a list of warning messages when:
/// - Plan-level sandbox configuration has potential security or usability concerns
pub fn check_sandbox_recommendations(metadata: &LoomMetadata) -> Vec<String> {
    let mut warnings = Vec::new();

    // Warn if "loom" is not in excluded_commands
    if !metadata
        .loom
        .sandbox
        .excluded_commands
        .contains(&"loom".to_string())
    {
        warnings.push(
            "Consider adding 'loom' to sandbox.excluded_commands to allow agents to use loom CLI"
                .to_string(),
        );
    }

    // Warn if allow_unsandboxed_escape is enabled (security concern)
    if metadata.loom.sandbox.allow_unsandboxed_escape {
        warnings.push(
            "Warning: allow_unsandboxed_escape=true reduces sandbox security. \
             Agents can escape sandbox restrictions with explicit commands."
                .to_string(),
        );
    }

    // Check for common misconfiguration: deny_write includes .work/** but that's already default
    let deny_write = &metadata.loom.sandbox.filesystem.deny_write;
    if deny_write.iter().any(|p| p.contains(".work")) {
        // This is actually the default, but if someone explicitly adds it, mention it
        warnings.push(
            "Note: .work/** is already denied by default in deny_write. \
             Explicit entry is redundant but harmless."
                .to_string(),
        );
    }

    warnings
}

/// Check for code-review-related recommendations (non-fatal warnings)
///
/// Returns a list of warning messages when:
/// - Plan contains code-review stages without proper configuration
pub fn check_code_review_recommendations(stages: &[super::types::StageDefinition]) -> Vec<String> {
    let mut warnings = Vec::new();

    // Find stages that are code-review type
    let code_review_stages: Vec<_> = stages
        .iter()
        .filter(|s| {
            s.stage_type == super::types::StageType::CodeReview
                || s.id.to_lowercase().contains("code-review")
                || s.name.to_lowercase().contains("code review")
        })
        .collect();

    // Warn if code-review stages have no dependencies (should review other stages' work)
    for stage in &code_review_stages {
        if stage.dependencies.is_empty() {
            warnings.push(format!(
                "Code review stage '{}' has no dependencies. \
                 Consider adding dependencies on stages whose code it should review.",
                stage.id
            ));
        }
    }

    warnings
}
