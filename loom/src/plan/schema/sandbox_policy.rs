//! Fail-closed policy for plan-authored sandbox expansion.

use super::types::{LoomMetadata, ValidationError};

pub(super) fn validate_excluded_commands(
    commands: &[String],
    errors: &mut Vec<ValidationError>,
    stage_id: Option<&str>,
) {
    if commands.is_empty() {
        return;
    }

    errors.push(ValidationError {
        message: format!(
            "sandbox.excluded_commands is not supported because command-prefix exclusions run \
             outside the host sandbox; remove these entries and use a trusted exact-argv broker: {}",
            commands.join(", ")
        ),
        stage_id: stage_id.map(str::to_string),
    });
}

/// Render policy changes that require an operator acknowledgement at init.
///
/// Command exclusions are rejected outright; acknowledgement applies only to
/// explicit sandbox disablement and unsandboxed escape.
pub fn unsafe_plan_reasons(metadata: &LoomMetadata) -> Vec<String> {
    let mut reasons = Vec::new();
    let plan_sandbox = &metadata.loom.sandbox;

    if !plan_sandbox.enabled {
        reasons.push("plan sandbox.enabled is false".to_string());
    }
    if plan_sandbox.allow_unsandboxed_escape {
        reasons.push("plan sandbox.allow_unsandboxed_escape is true".to_string());
    }

    for stage in &metadata.loom.stages {
        if stage.sandbox.enabled == Some(false) {
            reasons.push(format!("stage '{}' disables the sandbox", stage.id));
        }
        if stage.sandbox.allow_unsandboxed_escape == Some(true) {
            reasons.push(format!(
                "stage '{}' enables allow_unsandboxed_escape",
                stage.id
            ));
        }
    }

    reasons
}
