//! Knowledge command - manage curated codebase knowledge.
pub mod context;
pub mod sync;

use crate::fs::knowledge::{KnowledgeDir, KnowledgeTarget};
use crate::fs::work_dir::WorkDir;
use anyhow::{bail, Context, Result};
use colored::Colorize;

fn read_content_from_stdin() -> Result<String> {
    use std::io::Read;
    let limit = (crate::validation::MAX_KNOWLEDGE_CONTENT_LENGTH + 1) as u64;
    let mut buffer = String::new();
    std::io::stdin()
        .take(limit)
        .read_to_string(&mut buffer)
        .context("Failed to read from stdin")?;
    let trimmed = buffer.trim().to_string();
    if trimmed.is_empty() {
        bail!("No content received from stdin");
    }
    crate::validation::validate_knowledge_content(&trimmed)?;
    Ok(trimmed)
}

pub fn update(file: String, content: Option<String>) -> Result<()> {
    let content = match content {
        Some(c) if c == "-" => read_content_from_stdin()?,
        Some(c) => c,
        None => read_content_from_stdin()?,
    };

    crate::validation::validate_knowledge_content(&content)?;

    let work_dir = WorkDir::new(".")?;
    let project_root = work_dir
        .project_root()
        .context("Could not determine project root")?;
    let knowledge = KnowledgeDir::new(project_root);

    if !knowledge.exists() {
        knowledge
            .initialize()
            .context("Failed to initialize knowledge directory")?;
    }

    let target = KnowledgeTarget::parse(&file)?;
    knowledge.append_target(&target, &content)?;

    println!(
        "{} Appended to {}",
        "✓".green().bold(),
        target.display_name()
    );

    Ok(())
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "tests_legacy.rs"]
mod tests_legacy;
