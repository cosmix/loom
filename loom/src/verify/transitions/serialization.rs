//! Stage serialization and deserialization
//!
//! This module handles:
//! - Parsing stages from markdown with YAML frontmatter
//! - Serializing stages to markdown with YAML frontmatter

use anyhow::{Context, Result};

use crate::models::stage::Stage;
use crate::parser::frontmatter::extract_yaml_frontmatter;

/// Parse a Stage from markdown with YAML frontmatter
///
/// Expects content in the format:
/// ```markdown
/// ---
/// id: stage-1
/// name: Test Stage
/// ...
/// ---
///
/// # Stage body content
/// ```
pub fn parse_stage_from_markdown(content: &str) -> Result<Stage> {
    let frontmatter = extract_yaml_frontmatter(content)?;

    let stage: Stage = serde_yaml::from_value(frontmatter)
        .context("Failed to deserialize Stage from YAML frontmatter")?;

    Ok(stage)
}

/// Serialize a Stage to markdown with YAML frontmatter
///
/// Creates a markdown file with YAML frontmatter containing the stage data
/// followed by a markdown body with stage details.
pub fn serialize_stage_to_markdown(stage: &Stage) -> Result<String> {
    let yaml = serde_yaml::to_string(stage).context("Failed to serialize Stage to YAML")?;

    let mut content = String::new();
    content.push_str("---\n");
    content.push_str(&yaml);
    content.push_str("---\n\n");

    content.push_str(&format!("# Stage: {}\n\n", stage.name));

    if let Some(desc) = &stage.description {
        content.push_str(&format!("{desc}\n\n"));
    }

    content.push_str(&format!("**Status**: {:?}\n\n", stage.status));

    if !stage.dependencies.is_empty() {
        content.push_str("## Dependencies\n\n");
        for dep in &stage.dependencies {
            content.push_str(&format!("- {dep}\n"));
        }
        content.push('\n');
    }

    if !stage.acceptance.is_empty() {
        content.push_str("## Acceptance Criteria\n\n");
        for criterion in &stage.acceptance {
            content.push_str(&format!("- [ ] {criterion}\n"));
        }
        content.push('\n');
    }

    if !stage.files.is_empty() {
        content.push_str("## Files\n\n");
        for file in &stage.files {
            content.push_str(&format!("- `{file}`\n"));
        }
        content.push('\n');
    }

    Ok(content)
}
