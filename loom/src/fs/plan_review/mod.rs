//! Shared review generation for the CLI and plan finalization.
use crate::fs::memory::{list_journals, read_journal, MemoryEntry};
use crate::fs::work_dir::load_config;
use anyhow::{Context, Result};
use changes_section::append_changes_by_stage_section;
use chrono::Utc;
use sections::{append_open_findings, append_questions, append_spec_conformance};
use stages::{load_stage_infos, StageInfo};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};
/// Memory entries of each stage, keyed by stage id.
type Journals = HashMap<String, Vec<MemoryEntry>>;

mod changes_section;
mod sections;
mod stages;
mod summary;

pub use sections::{
    review_item_line, OPEN_FINDINGS_HEADING, OPEN_QUESTIONS_HEADING, SPEC_CONFORMANCE_HEADING,
    SPEC_CONFORMANCE_PREFIX,
};

/// Generate a review in an explicit checkout. Automatic finalization never requests AI.
pub fn generate(
    work_dir: &Path,
    project_root: &Path,
    output_root: &Path,
    ai_summary: bool,
) -> Result<PathBuf> {
    let config = load_config(work_dir)?.context("No active plan. Run 'loom init' first.")?;
    let plan_id = config.plan_id().unwrap_or("unknown");
    let plan_name = config.get_plan_str("plan_name").unwrap_or(plan_id);
    let description = plan_description(config.source_path(), project_root, ai_summary);
    let stages = load_stage_infos(&work_dir.join("stages"))?;
    let journals = load_journals(work_dir)?;
    let doc = render(plan_id, plan_name, &description, &stages, &journals);
    let plans_dir = output_root.join("doc/plans");
    fs::create_dir_all(&plans_dir).context("Failed to create doc/plans directory")?;
    let output = plans_dir.join(format!("REVIEW-{plan_id}.md"));
    crate::fs::locking::locked_write(&output, &doc)?;
    Ok(output)
}
fn plan_description(source: Option<PathBuf>, root: &Path, ai: bool) -> String {
    let Some(path) = source else {
        return "No plan description available.".to_string();
    };
    let path = if path.is_absolute() {
        path
    } else {
        root.join(path)
    };
    if ai {
        return summary::summarize_plan(&path);
    }
    fs::read_to_string(path)
        .map(|text| summary::fallback_description(&text))
        .unwrap_or_else(|_| "No plan description available.".to_string())
}
fn load_journals(work_dir: &Path) -> Result<Journals> {
    // Load all memory journals, keyed by stage_id
    let journal_names = list_journals(work_dir).context("Failed to list memory journals")?;

    let mut journals: Journals = HashMap::new();
    for stage_id in &journal_names {
        let journal = read_journal(work_dir, stage_id)
            .with_context(|| format!("Failed to read memory journal for stage '{stage_id}'"))?;
        if !journal.entries.is_empty() {
            journals.insert(stage_id.clone(), journal.entries);
        }
    }

    Ok(journals)
}
fn render(
    plan_id: &str,
    plan_name: &str,
    plan_description: &str,
    stages: &[StageInfo],
    journals: &Journals,
) -> String {
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
    doc.push_str(plan_description);
    doc.push_str("\n\n");

    // Changes by Stage
    append_changes_by_stage_section(&mut doc, stages, journals);

    append_spec_conformance(&mut doc, stages, journals);
    append_questions(&mut doc, stages, journals);
    append_open_findings(&mut doc, stages, journals);
    doc
}

#[cfg(test)]
mod tests;
