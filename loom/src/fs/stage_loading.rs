//! Shared utilities for loading stage definitions from .work/stages/ files.

use anyhow::{Context, Result};
use std::path::Path;

use crate::parser::frontmatter::parse_from_markdown;
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
    #[serde(default)]
    pub working_dir: Option<String>,
}

impl StageFrontmatter {
    /// Convert frontmatter to StageDefinition
    pub fn to_stage_definition(self) -> StageDefinition {
        StageDefinition {
            id: self.id,
            name: self.name,
            description: self.description,
            dependencies: self.dependencies,
            parallel_group: self.parallel_group,
            acceptance: self.acceptance,
            setup: self.setup,
            files: self.files,
            auto_merge: None,
            working_dir: self.working_dir.unwrap_or_else(|| ".".to_string()),
            stage_type: crate::plan::schema::StageType::default(),
            truths: Vec::new(),
            artifacts: Vec::new(),
            wiring: Vec::new(),
        }
    }
}

/// Extract YAML frontmatter from stage markdown file
///
/// Uses the canonical frontmatter parser which handles indentation-aware parsing
/// and embedded delimiters in YAML block scalars.
pub fn extract_stage_frontmatter(content: &str) -> Result<StageFrontmatter> {
    parse_from_markdown(content, "StageFrontmatter")
}

/// Load stage definitions from .work/stages/ directory
pub fn load_stages_from_work_dir(stages_dir: &Path) -> Result<Vec<StageDefinition>> {
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
        stages.push(frontmatter.to_stage_definition());
    }

    Ok(stages)
}
