//! YAML frontmatter parsing helpers

use anyhow::{Context, Result};

/// Extract YAML frontmatter from markdown content as a dynamic value
pub fn extract_yaml_frontmatter(content: &str) -> Result<serde_json::Value> {
    let yaml_content = extract_frontmatter_text(content)?;

    loom::yaml::from_str(&yaml_content).context("Failed to parse YAML frontmatter")
}

/// Extract the raw YAML text between the frontmatter delimiters
pub fn extract_frontmatter_text(content: &str) -> Result<String> {
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

    Ok(lines[1..end_idx].join("\n"))
}
