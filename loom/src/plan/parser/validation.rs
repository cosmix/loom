//! YAML parsing and metadata validation

use anyhow::{bail, Context, Result};

use crate::plan::schema::{validate, LoomMetadata};

/// Parse and validate YAML metadata
/// Returns the full LoomMetadata to allow callers to access sandbox config, etc.
pub fn parse_and_validate(yaml_content: &str) -> Result<LoomMetadata> {
    // Parse YAML
    let metadata: LoomMetadata =
        serde_yaml::from_str(yaml_content).with_context(|| "Failed to parse YAML metadata")?;

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
