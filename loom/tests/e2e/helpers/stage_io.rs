//! Stage file I/O helpers for tests

use anyhow::{Context, Result};
use loom::models::stage::Stage;
use std::path::Path;

use super::yaml::extract_yaml_frontmatter;

/// Writes a stage to .work/stages/{stage.id}.md
pub fn create_stage_file(work_dir: &Path, stage: &Stage) -> Result<()> {
    let stages_dir = work_dir.join(".work").join("stages");
    std::fs::create_dir_all(&stages_dir).context("Failed to create stages directory")?;

    let stage_path = stages_dir.join(format!("{}.md", stage.id));

    let yaml = serde_yaml::to_string(stage).context("Failed to serialize stage to YAML")?;

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

    std::fs::write(&stage_path, content)
        .with_context(|| format!("Failed to write stage file: {}", stage_path.display()))?;

    Ok(())
}

/// Reads a stage from .work/stages/{stage_id}.md
pub fn read_stage_file(work_dir: &Path, stage_id: &str) -> Result<Stage> {
    let stage_path = work_dir
        .join(".work")
        .join("stages")
        .join(format!("{stage_id}.md"));

    if !stage_path.exists() {
        anyhow::bail!("Stage file not found: {}", stage_path.display());
    }

    let content = std::fs::read_to_string(&stage_path)
        .with_context(|| format!("Failed to read stage file: {}", stage_path.display()))?;

    parse_stage_from_markdown(&content)
        .with_context(|| format!("Failed to parse stage from: {}", stage_path.display()))
}

/// Parse a Stage from markdown with YAML frontmatter
fn parse_stage_from_markdown(content: &str) -> Result<Stage> {
    let frontmatter = extract_yaml_frontmatter(content)?;

    let stage: Stage = serde_yaml::from_value(frontmatter)
        .context("Failed to deserialize Stage from YAML frontmatter")?;

    Ok(stage)
}
