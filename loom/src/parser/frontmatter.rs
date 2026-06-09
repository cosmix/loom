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

/// Extract the raw frontmatter text (without delimiters) from markdown content.
///
/// Matches the closing `---` at the same indentation as the opening delimiter,
/// so `---` inside YAML block scalars (which are indented) are not mistaken for
/// the closing delimiter.
///
/// # Returns
///
/// A `&str` slice of the YAML text between the two `---` delimiters.
///
/// # Errors
///
/// Returns an error if:
/// - Content is empty or missing opening `---`
/// - Closing `---` (at the same indentation) is not found
pub fn extract_frontmatter_raw(content: &str) -> Result<&str> {
    if content.is_empty() {
        bail!("No frontmatter delimiter found at start of content");
    }

    // Find the end of the first line (the opening delimiter)
    let first_newline = content.find('\n').unwrap_or(content.len());
    let first_line = &content[..first_newline];
    if !first_line.trim().starts_with("---") {
        bail!("No frontmatter delimiter found at start of content");
    }

    // Compute indentation of opening delimiter
    let opening_indent = first_line.len() - first_line.trim_start().len();

    // The frontmatter body starts after the first newline
    let body_start = if first_newline < content.len() {
        first_newline + 1
    } else {
        bail!("Frontmatter not properly closed with ---");
    };

    // Find closing delimiter at the same indentation
    let mut search_offset = body_start;
    loop {
        let remaining = &content[search_offset..];
        let line_end = remaining.find('\n').unwrap_or(remaining.len());
        let line = &remaining[..line_end];
        let trimmed = line.trim_start();
        if trimmed.starts_with("---") {
            let line_indent = line.len() - trimmed.len();
            if line_indent == opening_indent {
                // Return the slice between the opening and closing delimiters
                return Ok(&content[body_start..search_offset]);
            }
        }
        if line_end == remaining.len() {
            // Reached end of content without finding closing delimiter
            bail!("Frontmatter not properly closed with ---");
        }
        search_offset += line_end + 1;
    }
}

/// Extract YAML frontmatter from markdown content and deserialize it.
///
/// Delegates to [`extract_frontmatter_raw`] for the indentation-aware extraction,
/// then parses the resulting text as YAML.
///
/// # Errors
///
/// Returns an error if frontmatter extraction fails or YAML cannot be parsed.
pub fn extract_yaml_frontmatter(content: &str) -> Result<serde_yaml::Value> {
    let yaml_content = extract_frontmatter_raw(content)?;
    serde_yaml::from_str(yaml_content).context("Failed to parse YAML frontmatter")
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

    #[test]
    fn test_extract_frontmatter_raw_basic() {
        let content = "---\nkey: value\nstatus: Pending\n---\n# Content";
        let raw = extract_frontmatter_raw(content).unwrap();
        assert_eq!(raw, "key: value\nstatus: Pending\n");
    }

    #[test]
    fn test_extract_frontmatter_raw_embedded_delimiter() {
        // --- inside a block scalar must not be treated as the closing delimiter
        let content = "---\ndesc: |\n  ---\n  inner\n  ---\nstatus: ok\n---\n# body";
        let raw = extract_frontmatter_raw(content).unwrap();
        assert!(raw.contains("inner"));
        assert!(raw.contains("status: ok"));
    }

    #[test]
    fn test_extract_frontmatter_raw_no_opening() {
        let result = extract_frontmatter_raw("no frontmatter\n---\nfoo");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No frontmatter"));
    }

    #[test]
    fn test_extract_frontmatter_raw_no_closing() {
        let result = extract_frontmatter_raw("---\nkey: value\n# no close");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("not properly closed"));
    }

    #[test]
    fn test_extract_frontmatter_raw_empty() {
        let result = extract_frontmatter_raw("");
        assert!(result.is_err());
    }
}
