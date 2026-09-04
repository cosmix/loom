//! YAML parsing and metadata validation

use anyhow::{bail, Context, Result};

use crate::plan::schema::{validate, LoomMetadata};

/// Migration guidance shown for any field retired from the stage schema.
/// `truths` and `truth_checks` both moved into `acceptance`.
const RETIRED_TRUTHS_GUIDANCE: &str = "This field was removed from the stage schema. Move each \
     entry into `acceptance`, which takes either a plain string (command must exit 0) or an \
     object with `command:` plus `stdout_contains:` / `stdout_not_contains:` / `stderr_empty:` / \
     `exit_code:` for output matching. `before_stage` and `after_stage` still accept those same \
     check objects.";

/// Stage fields retired from the schema, mapped to migration guidance. Serde
/// renders a rejected retired field as a bare `unknown field `name`, expected
/// one of ...` with no hint about where the check went - pair each name with
/// that hint.
const RETIRED_STAGE_FIELDS: &[(&str, &str)] = &[
    ("truths", RETIRED_TRUTHS_GUIDANCE),
    ("truth_checks", RETIRED_TRUTHS_GUIDANCE),
];

/// Guidance for the first retired field named in a serde `unknown field`
/// error message, if any.
fn retired_field_guidance(error_message: &str) -> Option<&'static str> {
    RETIRED_STAGE_FIELDS
        .iter()
        .find(|(field, _)| error_message.contains(&format!("unknown field `{field}`")))
        .map(|(_, guidance)| *guidance)
}

/// Parse and validate YAML metadata
/// Returns the full LoomMetadata to allow callers to access sandbox config, etc.
pub fn parse_and_validate(yaml_content: &str) -> Result<LoomMetadata> {
    // Parse YAML. A retired field (e.g. `truths`) fails here with serde's raw
    // `unknown field` message, which includes the line/column - surface that
    // message plus migration guidance instead of swallowing it under the
    // generic context below.
    let metadata: LoomMetadata = match serde_yaml::from_str(yaml_content) {
        Ok(metadata) => metadata,
        Err(e) => {
            let message = e.to_string();
            if let Some(guidance) = retired_field_guidance(&message) {
                bail!("{message}\n\n{guidance}");
            }
            return Err(e).with_context(|| "Failed to parse YAML metadata");
        }
    };

    // Validate metadata
    if let Err(errors) = validate(&metadata) {
        let error_messages: Vec<_> = errors.iter().map(|e| e.to_string()).collect();
        bail!("Validation errors:\n  - {}", error_messages.join("\n  - "));
    }

    Ok(metadata)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_yaml() {
        let yaml = r#"
loom:
  version: 1
  stages:
    - id: stage-1
      name: "Test Stage"
      dependencies: []
      working_dir: "."
      acceptance:
        - "test -f README.md"
"#;
        let metadata = parse_and_validate(yaml).unwrap();
        let stages = &metadata.loom.stages;
        assert_eq!(stages.len(), 1);
        assert_eq!(stages[0].id, "stage-1");
    }

    #[test]
    fn test_parse_invalid_yaml() {
        let yaml = "this is: not: valid: yaml:::";
        assert!(parse_and_validate(yaml).is_err());
    }

    #[test]
    fn test_validate_invalid_dependency() {
        let yaml = r#"
loom:
  version: 1
  stages:
    - id: stage-1
      name: "Stage One"
      dependencies: ["nonexistent-stage"]
      working_dir: "."
"#;
        let result = parse_and_validate(yaml);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Unknown dependency"));
    }

    #[test]
    fn test_validate_self_dependency() {
        let yaml = r#"
loom:
  version: 1
  stages:
    - id: stage-1
      name: "Stage One"
      dependencies: ["stage-1"]
      working_dir: "."
"#;
        let result = parse_and_validate(yaml);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("cannot depend on itself"));
    }

    #[test]
    fn test_validate_empty_stage_name() {
        let yaml = r#"
loom:
  version: 1
  stages:
    - id: stage-1
      name: ""
      working_dir: "."
"#;
        let result = parse_and_validate(yaml);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be empty"));
    }

    #[test]
    fn test_validate_unsupported_version() {
        let yaml = r#"
loom:
  version: 2
  stages:
    - id: stage-1
      name: "Stage One"
      working_dir: "."
"#;
        let result = parse_and_validate(yaml);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Unsupported version"));
    }
}
