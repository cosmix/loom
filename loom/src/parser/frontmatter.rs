use anyhow::{bail, Context, Result};
use serde::de::DeserializeOwned;

/// Parse a type from markdown content with YAML frontmatter
///
/// Generic function that extracts YAML frontmatter and deserializes it into the target type.
///
/// # Example
///
/// ```text
/// let stage: Stage = parse_from_markdown(&content, "Stage")?;
/// let session: Session = parse_from_markdown(&content, "Session")?;
/// ```
///
/// # Errors
///
/// Returns an error if frontmatter extraction fails or YAML deserialization fails.
pub fn parse_from_markdown<T: DeserializeOwned>(content: &str, type_name: &str) -> Result<T> {
    let frontmatter = extract_yaml_frontmatter(content)?;
    serde_yaml::from_value(frontmatter)
        .with_context(|| format!("Failed to parse {type_name} from frontmatter"))
}

/// Extract a single field value from YAML frontmatter
///
/// This is a convenience function for extracting simple scalar values from frontmatter
/// without needing to deserialize the entire structure.
///
/// # Example
///
/// ```text
/// let stage_id = extract_frontmatter_field(&content, "stage_id")?;
/// let status = extract_frontmatter_field(&content, "status")?;
/// ```
///
/// Returns `None` if:
/// - The field is not found
/// - The field value is `null`, `~`, or empty
///
/// # Errors
///
/// Returns an error if frontmatter extraction fails.
pub fn extract_frontmatter_field(content: &str, field: &str) -> Result<Option<String>> {
    let yaml = extract_yaml_frontmatter(content)?;

    // Navigate to the field in the YAML value
    let value = match &yaml[field] {
        serde_yaml::Value::Null => return Ok(None),
        serde_yaml::Value::String(s) if s.is_empty() => return Ok(None),
        serde_yaml::Value::String(s) => s.clone(),
        serde_yaml::Value::Number(n) => n.to_string(),
        serde_yaml::Value::Bool(b) => b.to_string(),
        _ => return Ok(None),
    };

    Ok(Some(value))
}

/// Extract YAML frontmatter from markdown content
///
/// Expects frontmatter delimited by `---` at the start and end.
/// Returns the parsed YAML as a `serde_yaml::Value`.
///
/// # Example
///
/// ```text
/// ---
/// key: value
/// status: Pending
/// ---
/// # Markdown content here
/// ```
///
/// # Errors
///
/// Returns an error if:
/// - Content is empty or missing opening `---`
/// - Closing `---` is not found
/// - YAML content cannot be parsed
pub fn extract_yaml_frontmatter(content: &str) -> Result<serde_yaml::Value> {
    let lines: Vec<&str> = content.lines().collect();

    if lines.is_empty() || !lines[0].trim().starts_with("---") {
        bail!("No frontmatter delimiter found at start of content");
    }

    // Track indentation of opening delimiter to match closing delimiter at same level.
    // This prevents embedded `---` in YAML block scalars (which are indented) from
    // being mistakenly treated as the closing delimiter.
    let opening_indent = lines[0].len() - lines[0].trim_start().len();

    let mut end_idx = None;
    for (idx, line) in lines.iter().enumerate().skip(1) {
        let trimmed = line.trim_start();
        if trimmed.starts_with("---") {
            // Only match delimiter at the same indentation level as opening
            let line_indent = line.len() - trimmed.len();
            if line_indent == opening_indent {
                end_idx = Some(idx);
                break;
            }
        }
    }

    let end_idx =
        end_idx.ok_or_else(|| anyhow::anyhow!("Frontmatter not properly closed with ---"))?;

    let yaml_content = lines[1..end_idx].join("\n");

    serde_yaml::from_str(&yaml_content).context("Failed to parse YAML frontmatter")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_valid_frontmatter() {
        let content = r#"---
status: Pending
name: Test Stage
---
# Markdown content
More content here"#;

        let result = extract_yaml_frontmatter(content);
        assert!(result.is_ok());

        let yaml = result.unwrap();
        assert_eq!(yaml["status"].as_str(), Some("Pending"));
        assert_eq!(yaml["name"].as_str(), Some("Test Stage"));
    }

    #[test]
    fn test_extract_missing_opening_delimiter() {
        let content = "No frontmatter here\n# Just markdown";
        let result = extract_yaml_frontmatter(content);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No frontmatter delimiter"));
    }

    #[test]
    fn test_extract_missing_closing_delimiter() {
        let content = "---\nstatus: Pending\n# No closing delimiter";
        let result = extract_yaml_frontmatter(content);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("not properly closed"));
    }

    #[test]
    fn test_extract_empty_content() {
        let content = "";
        let result = extract_yaml_frontmatter(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_invalid_yaml() {
        let content = r#"---
invalid: yaml: syntax: error
---
# Content"#;
        let result = extract_yaml_frontmatter(content);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Failed to parse YAML"));
    }

    #[test]
    fn test_extract_with_embedded_delimiter_in_block_scalar() {
        // Test that `---` inside a YAML block scalar (indented) is not treated as closing delimiter
        let content = r#"---
id: test-stage
description: |
  Some description with example:

  ---
  name: example
  ---

  More text here.
status: completed
---
# Markdown content"#;

        let result = extract_yaml_frontmatter(content);
        assert!(result.is_ok(), "Failed to parse: {:?}", result.err());

        let yaml = result.unwrap();
        assert_eq!(yaml["id"].as_str(), Some("test-stage"));
        assert_eq!(yaml["status"].as_str(), Some("completed"));
        // Verify description contains the embedded ---
        let desc = yaml["description"].as_str().unwrap();
        assert!(
            desc.contains("---"),
            "Description should contain embedded ---"
        );
    }

    #[test]
    fn test_extract_with_whitespace_in_delimiters() {
        let content = r#"  ---
status: Ready
  ---
# Content"#;

        let result = extract_yaml_frontmatter(content);
        assert!(result.is_ok());
    }

    #[test]
    fn test_extract_frontmatter_field_string() {
        let content = r#"---
id: session-123
stage_id: my-stage
pid: 12345
status: running
---
# Content"#;

        assert_eq!(
            extract_frontmatter_field(content, "id").unwrap(),
            Some("session-123".to_string())
        );
        assert_eq!(
            extract_frontmatter_field(content, "stage_id").unwrap(),
            Some("my-stage".to_string())
        );
        assert_eq!(
            extract_frontmatter_field(content, "pid").unwrap(),
            Some("12345".to_string())
        );
        assert_eq!(
            extract_frontmatter_field(content, "status").unwrap(),
            Some("running".to_string())
        );
    }

    #[test]
    fn test_extract_frontmatter_field_nonexistent() {
        let content = r#"---
id: test
---
# Content"#;

        assert_eq!(
            extract_frontmatter_field(content, "nonexistent").unwrap(),
            None
        );
    }

    #[test]
    fn test_extract_frontmatter_field_null_values() {
        let content = r#"---
id: session-123
stage_id: null
pid: ~
empty_field:
---
# Content"#;

        assert_eq!(
            extract_frontmatter_field(content, "stage_id").unwrap(),
            None
        );
        assert_eq!(extract_frontmatter_field(content, "pid").unwrap(), None);
        assert_eq!(
            extract_frontmatter_field(content, "empty_field").unwrap(),
            None
        );
    }

    #[test]
    fn test_extract_frontmatter_field_bool_and_number() {
        let content = r#"---
merged: true
count: 42
---
# Content"#;

        assert_eq!(
            extract_frontmatter_field(content, "merged").unwrap(),
            Some("true".to_string())
        );
        assert_eq!(
            extract_frontmatter_field(content, "count").unwrap(),
            Some("42".to_string())
        );
    }
}
