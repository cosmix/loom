//! Plan document parser - extracts YAML metadata from markdown files

use anyhow::{Context, Result};
use std::path::Path;

mod extraction;
mod validation;

// Re-export functions for internal use
pub use extraction::{extract_plan_name, extract_yaml_metadata};
pub use validation::parse_and_validate;

/// Result of parsing a plan document
#[derive(Debug)]
pub struct ParsedPlan {
    pub id: String,
    pub name: String,
    pub source_path: String,
    pub stages: Vec<crate::plan::schema::StageDefinition>,
}

/// Parse a plan document and extract loom metadata
pub fn parse_plan(path: &Path) -> Result<ParsedPlan> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read plan file: {}", path.display()))?;

    parse_plan_content(&content, path)
}

/// Parse plan content (for testing without file system)
pub fn parse_plan_content(content: &str, source_path: &Path) -> Result<ParsedPlan> {
    // Extract plan name from first H1 header
    let name = extraction::extract_plan_name(content)?;

    // Generate plan ID from filename
    let id = source_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    // Extract YAML metadata block
    let yaml_content = extraction::extract_yaml_metadata(content)?;

    // Parse and validate YAML
    let stages = validation::parse_and_validate(&yaml_content)?;

    Ok(ParsedPlan {
        id,
        name,
        source_path: source_path.to_string_lossy().to_string(),
        stages,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_parse_plan_content() {
        let content = r#"
# PLAN: Test Plan

Description here.

---

<!-- loom METADATA - Do not edit manually -->

```yaml
loom:
  version: 1
  stages:
    - id: stage-1
      name: "Stage One"
      dependencies: []
      working_dir: "."
    - id: stage-2
      name: "Stage Two"
      dependencies: [stage-1]
      working_dir: "."
```

<!-- END loom METADATA -->
"#;

        let path = PathBuf::from("test-plan.md");
        let parsed = parse_plan_content(content, &path).unwrap();

        assert_eq!(parsed.name, "Test Plan");
        assert_eq!(parsed.stages.len(), 2);
        assert_eq!(parsed.stages[0].id, "stage-1");
        assert_eq!(parsed.stages[1].dependencies, vec!["stage-1"]);
    }

    #[test]
    fn test_parse_plan_with_all_fields() {
        let content = r#"
# Integration Test Plan

Complete test with all optional fields.

---

<!-- loom METADATA - Do not edit manually -->

```yaml
loom:
  version: 1
  stages:
    - id: stage-1
      name: "First Stage"
      description: "Initial setup"
      dependencies: []
      acceptance:
        - "cargo test"
      files:
        - "src/*.rs"
      working_dir: "."
    - id: stage-2
      name: "Second Stage"
      description: "Build on first stage"
      dependencies: ["stage-1"]
      parallel_group: "build"
      acceptance:
        - "cargo build"
        - "cargo clippy"
      files:
        - "Cargo.toml"
      working_dir: "."
```

<!-- END loom METADATA -->
"#;

        let path = PathBuf::from("integration-test.md");
        let parsed = parse_plan_content(content, &path).unwrap();

        assert_eq!(parsed.name, "Integration Test Plan");
        assert_eq!(parsed.id, "integration-test");
        assert_eq!(parsed.stages.len(), 2);

        // Verify first stage
        assert_eq!(parsed.stages[0].id, "stage-1");
        assert_eq!(parsed.stages[0].name, "First Stage");
        assert_eq!(
            parsed.stages[0].description,
            Some("Initial setup".to_string())
        );
        assert_eq!(parsed.stages[0].dependencies.len(), 0);
        assert_eq!(parsed.stages[0].acceptance, vec!["cargo test"]);
        assert_eq!(parsed.stages[0].files, vec!["src/*.rs"]);
        assert_eq!(parsed.stages[0].parallel_group, None);

        // Verify second stage
        assert_eq!(parsed.stages[1].id, "stage-2");
        assert_eq!(parsed.stages[1].name, "Second Stage");
        assert_eq!(parsed.stages[1].dependencies, vec!["stage-1"]);
        assert_eq!(parsed.stages[1].parallel_group, Some("build".to_string()));
        assert_eq!(parsed.stages[1].acceptance.len(), 2);
    }

    #[test]
    fn test_parse_invalid_yaml() {
        let content = r#"
# Plan

<!-- loom METADATA - Do not edit manually -->

```yaml
this is: not: valid: yaml:::
```

<!-- END loom METADATA -->
"#;
        let path = PathBuf::from("test.md");
        assert!(parse_plan_content(content, &path).is_err());
    }

    #[test]
    fn test_parse_validation_fails_invalid_dependency() {
        let content = r#"
# Plan

<!-- loom METADATA - Do not edit manually -->

```yaml
loom:
  version: 1
  stages:
    - id: stage-1
      name: "Stage One"
      dependencies: ["nonexistent-stage"]
      working_dir: "."
```

<!-- END loom METADATA -->
"#;
        let path = PathBuf::from("test.md");
        let result = parse_plan_content(content, &path);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Unknown dependency"));
    }

    #[test]
    fn test_parse_validation_fails_self_dependency() {
        let content = r#"
# Plan

<!-- loom METADATA - Do not edit manually -->

```yaml
loom:
  version: 1
  stages:
    - id: stage-1
      name: "Stage One"
      dependencies: ["stage-1"]
      working_dir: "."
```

<!-- END loom METADATA -->
"#;
        let path = PathBuf::from("test.md");
        let result = parse_plan_content(content, &path);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("cannot depend on itself"));
    }

    #[test]
    fn test_parse_validation_fails_empty_stage_name() {
        let content = r#"
# Plan

<!-- loom METADATA - Do not edit manually -->

```yaml
loom:
  version: 1
  stages:
    - id: stage-1
      name: ""
      working_dir: "."
```

<!-- END loom METADATA -->
"#;
        let path = PathBuf::from("test.md");
        let result = parse_plan_content(content, &path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be empty"));
    }

    #[test]
    fn test_parse_validation_fails_unsupported_version() {
        let content = r#"
# Plan

<!-- loom METADATA - Do not edit manually -->

```yaml
loom:
  version: 2
  stages:
    - id: stage-1
      name: "Stage One"
      working_dir: "."
```

<!-- END loom METADATA -->
"#;
        let path = PathBuf::from("test.md");
        let result = parse_plan_content(content, &path);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Unsupported version"));
    }
}
