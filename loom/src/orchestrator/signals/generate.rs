use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::handoff::git_handoff::GitHistory;
use crate::models::session::Session;
use crate::models::stage::Stage;
use crate::models::worktree::Worktree;

use super::format::format_signal_content;
use super::types::{DependencyStatus, EmbeddedContext};

pub fn generate_signal(
    session: &Session,
    stage: &Stage,
    worktree: &Worktree,
    dependencies_status: &[DependencyStatus],
    handoff_file: Option<&str>,
    git_history: Option<&GitHistory>,
    work_dir: &Path,
) -> Result<PathBuf> {
    let signals_dir = work_dir.join("signals");

    if !signals_dir.exists() {
        fs::create_dir_all(&signals_dir).context("Failed to create signals directory")?;
    }

    // Build embedded context by reading files
    let embedded_context = build_embedded_context(work_dir, handoff_file);

    let signal_path = signals_dir.join(format!("{}.md", session.id));
    let content = format_signal_content(
        session,
        stage,
        worktree,
        dependencies_status,
        handoff_file,
        git_history,
        &embedded_context,
    );

    fs::write(&signal_path, content)
        .with_context(|| format!("Failed to write signal file: {}", signal_path.display()))?;

    Ok(signal_path)
}

/// Build embedded context by reading handoff, structure.md, and plan overview files
pub(super) fn build_embedded_context(
    work_dir: &Path,
    handoff_file: Option<&str>,
) -> EmbeddedContext {
    let mut context = EmbeddedContext::default();

    // Read handoff content if specified
    if let Some(handoff_name) = handoff_file {
        let handoff_path = work_dir.join("handoffs").join(format!("{handoff_name}.md"));
        if handoff_path.exists() {
            context.handoff_content = fs::read_to_string(&handoff_path).ok();
        }
    }

    // Read structure.md if it exists
    let structure_path = work_dir.join("structure.md");
    if structure_path.exists() {
        context.structure_content = fs::read_to_string(&structure_path).ok();
    }

    // Read plan overview from config.toml and the plan file
    context.plan_overview = read_plan_overview(work_dir);

    context
}

/// Read the plan overview from the plan file referenced in config.toml
fn read_plan_overview(work_dir: &Path) -> Option<String> {
    let config_path = work_dir.join("config.toml");
    if !config_path.exists() {
        return None;
    }

    let config_content = fs::read_to_string(&config_path).ok()?;
    let config: toml::Value = config_content.parse().ok()?;

    let source_path = config.get("plan")?.get("source_path")?.as_str()?;

    let plan_path = PathBuf::from(source_path);
    if !plan_path.exists() {
        return None;
    }

    let plan_content = fs::read_to_string(&plan_path).ok()?;

    // Extract overview section from plan markdown
    extract_plan_overview(&plan_content)
}

/// Extract overview and proposed changes sections from plan markdown
pub(super) fn extract_plan_overview(plan_content: &str) -> Option<String> {
    let mut overview = String::new();
    let mut in_relevant_section = false;
    let mut current_section = String::new();

    for line in plan_content.lines() {
        // Detect section headers
        if line.starts_with("## ") {
            let section_name = line.trim_start_matches("## ").trim().to_lowercase();

            // Save accumulated content from previous relevant section
            if in_relevant_section && !current_section.is_empty() {
                overview.push_str(&current_section);
                overview.push_str("\n\n");
                current_section.clear();
            }

            // Check if entering a relevant section
            in_relevant_section = section_name.contains("overview")
                || section_name.contains("proposed changes")
                || section_name.contains("summary")
                || section_name.contains("current state");

            if in_relevant_section {
                current_section.push_str(line);
                current_section.push('\n');
            }
        } else if line.starts_with("# ") && overview.is_empty() {
            // Capture plan title
            overview.push_str(line);
            overview.push_str("\n\n");
        } else if in_relevant_section {
            // Stop at next major section (Stages, metadata, etc.)
            let trimmed = line.trim().to_lowercase();
            if trimmed.starts_with("## stages")
                || trimmed.starts_with("```yaml")
                || trimmed.starts_with("<!-- loom")
            {
                in_relevant_section = false;
                if !current_section.is_empty() {
                    overview.push_str(&current_section);
                    overview.push_str("\n\n");
                    current_section.clear();
                }
            } else {
                current_section.push_str(line);
                current_section.push('\n');
            }
        }
    }

    // Capture any remaining content
    if in_relevant_section && !current_section.is_empty() {
        overview.push_str(&current_section);
    }

    let trimmed = overview.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
