use anyhow::{Context, Result};

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
        anyhow::bail!("No frontmatter delimiter found at start of content");
    }

    let mut end_idx = None;
    for (idx, line) in lines.iter().enumerate().skip(1) {
        if line.trim().starts_with("---") {
            end_idx = Some(idx);
            break;
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
    fn test_extract_with_whitespace_in_delimiters() {
        let content = r#"  ---
status: Ready
  ---
# Content"#;

        let result = extract_yaml_frontmatter(content);
        assert!(result.is_ok());
    }
}
