//! Generate a structured code review document from stage memory journals.

use anyhow::{Context, Result};
use chrono::Utc;
use colored::Colorize;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::fs::memory::{list_journals, read_journal, MemoryEntry, MemoryEntryType};
use crate::fs::work_dir::load_config;
use crate::git::worktree::{find_repo_root_from_cwd, find_worktree_root_from_cwd};
use crate::parser::frontmatter::extract_frontmatter_field;

/// Find the .work directory, handling both worktree and main repo contexts.
fn get_work_dir() -> Result<PathBuf> {
    let cwd = env::current_dir().context("Failed to get current directory")?;

    // Check if we're in a worktree first
    if let Some(worktree_root) = find_worktree_root_from_cwd(&cwd) {
        let work_dir = worktree_root.join(".work");
        if work_dir.exists() {
            return Ok(work_dir);
        }
    }

    // Not in a worktree — find the repo root
    if let Some(repo_root) = find_repo_root_from_cwd(&cwd) {
        let work_dir = repo_root.join(".work");
        if work_dir.exists() {
            return Ok(work_dir);
        }
    }

    // Fallback: check current directory
    let work_dir = cwd.join(".work");
    if work_dir.exists() {
        return Ok(work_dir);
    }

    anyhow::bail!(".work directory not found. Run 'loom init' first.");
}

/// Information extracted from a stage file.
struct StageInfo {
    id: String,
    name: String,
    description: String,
    status: String,
    /// Original filename, used for sorting.
    _filename: String,
}

/// Load all stage files from `.work/stages/` sorted by filename.
fn load_stage_infos(stages_dir: &Path) -> Result<Vec<StageInfo>> {
    if !stages_dir.exists() {
        return Ok(Vec::new());
    }

    let mut entries: Vec<(String, PathBuf)> = fs::read_dir(stages_dir)
        .context("Failed to read stages directory")?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|s| s.to_str())
                .is_some_and(|ext| ext == "md")
        })
        .map(|e| {
            let filename = e.file_name().to_string_lossy().to_string();
            (filename, e.path())
        })
        .collect();

    // Sort by filename so depth-prefixed names come out in topological order
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let mut stages = Vec::new();
    for (filename, path) in entries {
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read stage file: {}", path.display()))?;

        let id = extract_frontmatter_field(&content, "id")
            .ok()
            .flatten()
            .unwrap_or_else(|| filename.trim_end_matches(".md").to_string());

        let name = extract_frontmatter_field(&content, "name")
            .ok()
            .flatten()
            .unwrap_or_else(|| id.clone());

        let description = extract_frontmatter_field(&content, "description")
            .ok()
            .flatten()
            .unwrap_or_default();

        let status = extract_frontmatter_field(&content, "status")
            .ok()
            .flatten()
            .unwrap_or_else(|| "Unknown".to_string());

        stages.push(StageInfo {
            id,
            name,
            description,
            status,
            _filename: filename,
        });
    }

    Ok(stages)
}

/// Extract plan description from the plan markdown file.
///
/// Tries to read the text between the first `#` heading and the
/// `<!-- loom METADATA -->` marker. Falls back to a default message.
fn extract_plan_description(plan_path: &Path) -> String {
    let content = match fs::read_to_string(plan_path) {
        Ok(c) => c,
        Err(_) => return "No plan description available.".to_string(),
    };

    // Find the loom metadata marker
    let metadata_marker = "<!-- loom METADATA -->";
    let body = if let Some(idx) = content.find(metadata_marker) {
        &content[..idx]
    } else {
        &content
    };

    // Skip past the first `#` heading line
    let mut lines = body.lines().peekable();
    while let Some(line) = lines.peek() {
        if line.trim_start().starts_with('#') {
            lines.next(); // consume the heading
            break;
        }
        lines.next();
    }

    // Collect remaining non-empty lines as the description
    let description: Vec<&str> = lines.collect();
    let trimmed = description.join("\n").trim().to_string();

    if trimmed.is_empty() {
        "No plan description available.".to_string()
    } else {
        trimmed
    }
}

/// Format a single bullet point for a memory entry.
fn format_entry_bullet(entry: &MemoryEntry) -> String {
    match &entry.context {
        Some(ctx) => format!("- {} *({})*", entry.content, ctx),
        None => format!("- {}", entry.content),
    }
}

/// Render the "Changes by Stage" section for one stage.
fn render_stage_section(stage: &StageInfo, entries: &[&MemoryEntry]) -> String {
    let mut out = String::new();

    out.push_str(&format!("### {} ({})\n\n", stage.name, stage.id));
    out.push_str(&format!("**Status:** {}  \n", stage.status));
    if !stage.description.is_empty() {
        out.push_str(&format!("**Purpose:** {}\n\n", stage.description));
    } else {
        out.push('\n');
    }

    // Files Changed
    out.push_str("#### Files Changed\n\n");
    let changes: Vec<&MemoryEntry> = entries
        .iter()
        .copied()
        .filter(|e| e.entry_type == MemoryEntryType::Change)
        .collect();
    if changes.is_empty() {
        out.push_str("No changes recorded.\n");
    } else {
        for entry in &changes {
            out.push_str(&format!("{}\n", format_entry_bullet(entry)));
        }
    }

    // Key Decisions
    out.push_str("\n#### Key Decisions\n\n");
    let decisions: Vec<&MemoryEntry> = entries
        .iter()
        .copied()
        .filter(|e| e.entry_type == MemoryEntryType::Decision)
        .collect();
    if decisions.is_empty() {
        out.push_str("No decisions recorded.\n");
    } else {
        for entry in &decisions {
            out.push_str(&format!("{}\n", format_entry_bullet(entry)));
        }
    }

    // Notes (most recent 10)
    out.push_str("\n#### Notes\n\n");
    let notes: Vec<&MemoryEntry> = entries
        .iter()
        .copied()
        .filter(|e| e.entry_type == MemoryEntryType::Note)
        .collect();
    if notes.is_empty() {
        out.push_str("No notes recorded.\n");
    } else {
        for entry in notes.iter().rev().take(10) {
            out.push_str(&format!("{}\n", format_entry_bullet(entry)));
        }
    }

    out.push('\n');
    out
}

/// Execute the `loom review` command.
pub fn execute() -> Result<()> {
    let work_dir = get_work_dir()?;

    // Resolve the project root (follow symlinks for worktrees)
    let project_root: PathBuf = {
        let work_dir_struct = crate::fs::work_dir::WorkDir::new(
            work_dir
                .parent()
                .context("Failed to determine parent of .work directory")?,
        )?;
        work_dir_struct
            .main_project_root()
            .context("Could not determine project root")?
    };

    // Load config.toml
    let config = load_config(&work_dir)?.context("No active plan. Run 'loom init' first.")?;

    let plan_id = config
        .plan_id()
        .unwrap_or("unknown")
        .to_string();

    let plan_name = config
        .get_plan_str("plan_name")
        .unwrap_or(&plan_id)
        .to_string();

    let source_path = config.source_path();

    println!(
        "{} Generating code review for plan '{}'...",
        "→".cyan().bold(),
        plan_name.bold()
    );

    // Extract plan description from the plan file
    let plan_description = match &source_path {
        Some(path) => {
            // source_path may be relative to project root or absolute
            let resolved = if path.is_absolute() {
                path.clone()
            } else {
                project_root.join(path)
            };
            extract_plan_description(&resolved)
        }
        None => "No plan description available.".to_string(),
    };

    // Load stage infos
    let stages_dir = work_dir.join("stages");
    let stages = load_stage_infos(&stages_dir)
        .context("Failed to load stage files")?;

    // Load all memory journals, keyed by stage_id
    let journal_names = list_journals(&work_dir).context("Failed to list memory journals")?;

    let mut journals: HashMap<String, Vec<MemoryEntry>> = HashMap::new();
    for stage_id in &journal_names {
        let journal = read_journal(&work_dir, stage_id)
            .with_context(|| format!("Failed to read memory journal for stage '{stage_id}'"))?;
        if !journal.entries.is_empty() {
            journals.insert(stage_id.clone(), journal.entries);
        }
    }

    // Generate the review document
    let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();

    let mut doc = String::new();
    doc.push_str(&format!("# Code Review: {}\n\n", plan_name));
    doc.push_str(&format!(
        "**Plan:** {} | **Generated:** {}\n\n",
        plan_id, timestamp
    ));

    // Summary section
    doc.push_str("## Summary\n\n");
    doc.push_str(&plan_description);
    doc.push_str("\n\n");

    // Changes by Stage
    doc.push_str("## Changes by Stage\n\n");

    let mut has_any_stage = false;
    for stage in &stages {
        let entries_opt = journals.get(&stage.id);
        let entries: Vec<&MemoryEntry> = entries_opt
            .map(|v| v.iter().collect())
            .unwrap_or_default();

        // Skip stages with no memory entries
        if entries.is_empty() {
            continue;
        }

        has_any_stage = true;
        doc.push_str(&render_stage_section(stage, &entries));
    }

    if !has_any_stage {
        doc.push_str("No stage memory recorded.\n\n");
    }

    // Open Questions — collect all question entries across all stages
    doc.push_str("## Open Questions\n\n");

    let mut all_questions: Vec<(&str, &MemoryEntry)> = Vec::new();
    for stage in &stages {
        if let Some(entries) = journals.get(&stage.id) {
            for entry in entries {
                if entry.entry_type == MemoryEntryType::Question {
                    all_questions.push((&stage.name, entry));
                }
            }
        }
    }

    if all_questions.is_empty() {
        doc.push_str("No open questions.\n");
    } else {
        for (stage_name, entry) in &all_questions {
            doc.push_str(&format!("- **[{}]** {}\n", stage_name, entry.content));
        }
    }

    doc.push('\n');

    // Write to doc/plans/REVIEW-{plan_id}.md
    let plans_dir = project_root.join("doc").join("plans");
    fs::create_dir_all(&plans_dir).context("Failed to create doc/plans directory")?;

    let output_filename = format!("REVIEW-{}.md", plan_id);
    let output_path = plans_dir.join(&output_filename);

    fs::write(&output_path, &doc)
        .with_context(|| format!("Failed to write review document: {}", output_path.display()))?;

    println!(
        "{} Review document written to {}",
        "✓".green().bold(),
        output_path
            .strip_prefix(&project_root)
            .unwrap_or(&output_path)
            .display()
            .to_string()
            .cyan()
    );

    Ok(())
}
