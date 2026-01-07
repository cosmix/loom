//! Plan document parser - extracts YAML metadata from markdown files

use anyhow::{bail, Context, Result};
use std::path::Path;

use super::schema::{validate, LoomMetadata, StageDefinition};

/// Result of parsing a plan document
#[derive(Debug)]
pub struct ParsedPlan {
    pub id: String,
    pub name: String,
    pub source_path: String,
    pub stages: Vec<StageDefinition>,
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
    let name = extract_plan_name(content)?;

    // Generate plan ID from filename
    let id = source_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    // Extract YAML metadata block
    let yaml_content = extract_yaml_metadata(content)?;

    // Parse YAML
    let metadata: LoomMetadata =
        serde_yaml::from_str(&yaml_content).with_context(|| "Failed to parse YAML metadata")?;

    // Validate metadata
    if let Err(errors) = validate(&metadata) {
        let error_messages: Vec<_> = errors.iter().map(|e| e.to_string()).collect();
        bail!("Validation errors:\n  - {}", error_messages.join("\n  - "));
    }

    Ok(ParsedPlan {
        id,
        name,
        source_path: source_path.to_string_lossy().to_string(),
        stages: metadata.loom.stages,
    })
}

/// Extract plan name from first H1 header
fn extract_plan_name(content: &str) -> Result<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("# ") {
            let name = trimmed.trim_start_matches("# ").trim();
            // Remove "PLAN:" prefix if present
            let name = name.strip_prefix("PLAN:").unwrap_or(name).trim();
            return Ok(name.to_string());
        }
    }
    bail!("No H1 header found in plan document")
}

/// Extract YAML content from metadata block
fn extract_yaml_metadata(content: &str) -> Result<String> {
    // Find the metadata markers
    let start_marker = "<!-- loom METADATA";
    let end_marker = "<!-- END loom METADATA";

    let start_pos = content
        .find(start_marker)
        .ok_or_else(|| anyhow::anyhow!("No loom METADATA block found"))?;

    let end_pos = content
        .find(end_marker)
        .ok_or_else(|| anyhow::anyhow!("No END loom METADATA marker found"))?;

    if end_pos <= start_pos {
        bail!("Invalid metadata block: END marker before START");
    }

    let metadata_section = &content[start_pos..end_pos];

    // Find YAML code block within metadata section
    let yaml_start = metadata_section
        .find("```yaml")
        .ok_or_else(|| anyhow::anyhow!("No ```yaml block in metadata"))?;

    let yaml_content_start = yaml_start + "```yaml".len();

    let yaml_end = metadata_section[yaml_content_start..]
        .find("```")
        .ok_or_else(|| anyhow::anyhow!("No closing ``` for YAML block"))?;

    let yaml_content = &metadata_section[yaml_content_start..yaml_content_start + yaml_end];

    Ok(yaml_content.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_extract_plan_name() {
        let content = "# PLAN: Test Plan\n\nSome content";
        assert_eq!(extract_plan_name(content).unwrap(), "Test Plan");

        let content2 = "# My Plan\n\nContent";
        assert_eq!(extract_plan_name(content2).unwrap(), "My Plan");
    }

    #[test]
    fn test_extract_yaml_metadata() {
        let content = r#"
# Test Plan

Some content

---

<!-- loom METADATA - Do not edit manually -->

```yaml
loom:
  version: 1
  stages:
    - id: stage-1
      name: "Test Stage"
```

<!-- END loom METADATA -->
"#;

        let yaml = extract_yaml_metadata(content).unwrap();
        assert!(yaml.contains("loom:"));
        assert!(yaml.contains("stage-1"));
    }

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
    - id: stage-2
      name: "Stage Two"
      dependencies: [stage-1]
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
    fn test_extract_plan_name_no_header() {
        let content = "Some content without header";
        assert!(extract_plan_name(content).is_err());
    }

    #[test]
    fn test_extract_yaml_no_metadata_block() {
        let content = "# Plan\n\nNo metadata here";
        assert!(extract_yaml_metadata(content).is_err());
    }

    #[test]
    fn test_extract_yaml_no_end_marker() {
        let content = r#"
# Plan

<!-- loom METADATA - Do not edit manually -->

```yaml
loom:
  version: 1
```
"#;
        assert!(extract_yaml_metadata(content).is_err());
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
