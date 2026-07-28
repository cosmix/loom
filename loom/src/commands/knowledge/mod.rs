//! Knowledge command - manage curated codebase knowledge.
pub mod audit;
pub mod bootstrap;
pub mod check;
pub mod gc;
pub mod spawn;

use crate::fs::knowledge::{KnowledgeDir, KnowledgeFile, KnowledgeLayout, KnowledgeTarget};
use crate::fs::work_dir::WorkDir;
use anyhow::{bail, Context, Result};
use colored::Colorize;

pub fn show(file: Option<String>) -> Result<()> {
    let work_dir = WorkDir::new(".")?;
    let project_root = work_dir
        .project_root()
        .context("Could not determine project root")?;
    let knowledge = KnowledgeDir::new(project_root);

    if !knowledge.exists() {
        println!(
            "{} Knowledge directory not found. Run 'loom knowledge init' to create it.",
            "─".dimmed()
        );
        return Ok(());
    }

    match file {
        Some(target_name) => {
            let target = KnowledgeTarget::parse(&target_name)?;
            let content = knowledge.read_target(&target)?;
            println!("{content}");
        }
        None => match knowledge.layout() {
            KnowledgeLayout::Hierarchical => {
                let index = knowledge.read_index()?;
                println!("{index}");
            }
            KnowledgeLayout::Legacy => {
                let summary = knowledge.generate_summary()?;
                if summary.is_empty() {
                    println!("{} No knowledge files have content yet.", "─".dimmed());
                } else {
                    println!("{summary}");
                }
            }
        },
    }

    Ok(())
}

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

pub fn replace_section(file: String, heading: String, content: Option<String>) -> Result<()> {
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
        bail!("Knowledge directory not found. Run 'loom knowledge init' first.");
    }

    let target = KnowledgeTarget::parse(&file)?;
    knowledge.replace_section_target(&target, &heading, &content)?;

    println!(
        "{} Replaced section '{}' in {}",
        "✓".green().bold(),
        heading,
        target.display_name()
    );

    Ok(())
}

pub fn init() -> Result<()> {
    let work_dir = WorkDir::new(".")?;
    let project_root = work_dir
        .project_root()
        .context("Could not determine project root")?;
    let knowledge = KnowledgeDir::new(project_root);

    if knowledge.exists() {
        println!(
            "{} Knowledge directory already exists at {}",
            "─".dimmed(),
            knowledge.root().display()
        );
        return Ok(());
    }

    // `initialize()` writes INDEX.md for a freshly created directory, so new
    // projects start hierarchical here and via `loom init` / `loom map` alike.
    knowledge.initialize()?;

    println!("{} Initialized knowledge directory", "✓".green().bold());
    println!();
    println!("Created files:");
    for file_type in KnowledgeFile::all() {
        println!("  {} - {}", file_type.filename(), file_type.description());
    }

    Ok(())
}

/// Regenerate `INDEX.md`. On a legacy (flat) directory this is the opt-in
/// upgrade path — it creates `INDEX.md` for the first time, turning the
/// directory hierarchical.
pub fn index() -> Result<()> {
    let work_dir = WorkDir::new(".")?;
    let project_root = work_dir
        .project_root()
        .context("Could not determine project root")?;
    let knowledge = KnowledgeDir::new(project_root);

    if !knowledge.exists() {
        bail!("Knowledge directory not found. Run 'loom knowledge init' first.");
    }

    let was_legacy = knowledge.layout() == KnowledgeLayout::Legacy;

    knowledge.write_index()?;

    let tier1_count = knowledge.list_files()?.len();
    let topic_count = knowledge.list_topics()?.len();

    if was_legacy {
        println!(
            "{} Upgraded knowledge directory to the hierarchical layout (created INDEX.md)",
            "✓".green().bold()
        );
    } else {
        println!("{} Regenerated INDEX.md", "✓".green().bold());
    }
    println!(
        "  Indexed {} tier-1 file{} and {} topic{}",
        tier1_count,
        if tier1_count == 1 { "" } else { "s" },
        topic_count,
        if topic_count == 1 { "" } else { "s" },
    );

    Ok(())
}

pub fn list() -> Result<()> {
    let work_dir = WorkDir::new(".")?;
    let project_root = work_dir
        .project_root()
        .context("Could not determine project root")?;
    let knowledge = KnowledgeDir::new(project_root);

    if !knowledge.exists() {
        println!(
            "{} Knowledge directory not found. Run 'loom knowledge init' to create it.",
            "─".dimmed()
        );
        return Ok(());
    }

    let files = knowledge.list_files()?;

    if files.is_empty() {
        println!("{} No knowledge files found.", "─".dimmed());
        return Ok(());
    }

    println!("{}", "Knowledge Files".bold());
    println!();

    for (file_type, path) in files {
        let content = std::fs::read_to_string(&path).ok();
        let line_count = content.as_ref().map(|c| c.lines().count()).unwrap_or(0);

        println!(
            "  {} {} ({} lines)",
            "─".dimmed(),
            file_type.filename().cyan(),
            line_count
        );
        println!("    {}", file_type.description().dimmed());
    }

    println!();
    println!("{}", "Topics".bold());
    println!();

    let topics = knowledge.list_topics()?;
    if topics.is_empty() {
        println!("  {} No topics yet.", "─".dimmed());
    } else {
        for category in KnowledgeFile::all() {
            let category_topics: Vec<_> =
                topics.iter().filter(|t| t.category == *category).collect();
            if category_topics.is_empty() {
                continue;
            }
            println!("  {}", category.dir_name().cyan());
            for topic in category_topics {
                println!(
                    "    {} {}/{}.md ({} lines)",
                    "─".dimmed(),
                    category.dir_name(),
                    topic.slug,
                    topic.line_count
                );
                if !topic.title.is_empty() {
                    println!("      {}", topic.title.dimmed());
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "tests_legacy.rs"]
mod tests_legacy;
