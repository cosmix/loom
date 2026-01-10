//! YAML frontmatter parsing for stage files.

use anyhow::{bail, Context, Result};
use std::path::PathBuf;

use crate::plan::schema::StageDefinition;

/// Stage frontmatter data extracted from .work/stages/*.md files
#[derive(Debug, serde::Deserialize)]
pub struct StageFrontmatter {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub parallel_group: Option<String>,
    #[serde(default)]
    pub acceptance: Vec<String>,
    #[serde(default)]
    pub setup: Vec<String>,
    #[serde(default)]
    pub files: Vec<String>,
}

/// Extract YAML frontmatter from stage markdown file
pub fn extract_stage_frontmatter(content: &str) -> Result<StageFrontmatter> {
    let lines: Vec<&str> = content.lines().collect();

    if lines.is_empty() || !lines[0].trim().starts_with("---") {
        bail!("No frontmatter delimiter found");
    }

    let mut end_idx = None;
    for (idx, line) in lines.iter().enumerate().skip(1) {
        if line.trim().starts_with("---") {
            end_idx = Some(idx);
            break;
        }
    }

    let end_idx = end_idx.ok_or_else(|| anyhow::anyhow!("Frontmatter not properly closed"))?;

    let yaml_content = lines[1..end_idx].join("\n");

    serde_yaml::from_str(&yaml_content).context("Failed to parse stage YAML frontmatter")
}

/// Load stage definitions from .work/stages/ directory
pub fn load_stages_from_work_dir(stages_dir: &PathBuf) -> Result<Vec<StageDefinition>> {
    let mut stages = Vec::new();

    for entry in std::fs::read_dir(stages_dir)
        .with_context(|| format!("Failed to read stages directory: {}", stages_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();

        // Skip non-markdown files
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }

        // Read and parse the stage file
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read stage file: {}", path.display()))?;

        // Extract YAML frontmatter
        let frontmatter = match extract_stage_frontmatter(&content) {
            Ok(fm) => fm,
            Err(e) => {
                eprintln!("Warning: Could not parse {}: {}", path.display(), e);
                continue;
            }
        };

        // Convert to StageDefinition
        let stage_def = StageDefinition {
            id: frontmatter.id,
            name: frontmatter.name,
            description: frontmatter.description,
            dependencies: frontmatter.dependencies,
            parallel_group: frontmatter.parallel_group,
            acceptance: frontmatter.acceptance,
            setup: frontmatter.setup,
            files: frontmatter.files,
            auto_merge: None,
        };

        stages.push(stage_def);
    }

    Ok(stages)
}
