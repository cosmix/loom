//! YAML frontmatter parsing for stage files.

use anyhow::{anyhow, bail, Context, Result};

use crate::models::stage::Stage;

/// Parse a Stage from markdown with YAML frontmatter
pub fn parse_stage_from_markdown(content: &str) -> Result<Stage> {
    let frontmatter = extract_yaml_frontmatter(content)?;

    let stage: Stage = serde_yaml::from_value(frontmatter)
        .context("Failed to deserialize stage from YAML frontmatter")?;

    Ok(stage)
}

/// Extract YAML frontmatter from markdown content
pub fn extract_yaml_frontmatter(content: &str) -> Result<serde_yaml::Value> {
    let lines: Vec<&str> = content.lines().collect();

    if lines.is_empty() || !lines[0].starts_with("---") {
        bail!("Missing YAML frontmatter delimiter");
    }

    let end_index = lines
        .iter()
        .skip(1)
        .position(|line| line.starts_with("---"))
        .ok_or_else(|| anyhow!("Missing closing YAML frontmatter delimiter"))?
        + 1;

    let yaml_lines = &lines[1..end_index];
    let yaml_content = yaml_lines.join("\n");

    serde_yaml::from_str(&yaml_content).context("Failed to parse YAML frontmatter")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_yaml_frontmatter() {
        let content = r#"---
id: test-id
name: Test Name
---

# Content here
"#;

        let yaml = extract_yaml_frontmatter(content).expect("Should extract YAML");
        let id = yaml["id"].as_str().unwrap();
        assert_eq!(id, "test-id");
    }

    #[test]
    fn test_extract_yaml_frontmatter_missing() {
        let content = "# No frontmatter here";
        let result = extract_yaml_frontmatter(content);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Missing YAML frontmatter"));
    }
}
