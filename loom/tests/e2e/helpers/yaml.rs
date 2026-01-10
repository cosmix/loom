//! YAML frontmatter parsing helpers

use anyhow::{Context, Result};

/// Extract YAML frontmatter from markdown content
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
