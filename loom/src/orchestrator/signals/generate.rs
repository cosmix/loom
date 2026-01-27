use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::fs::knowledge::KnowledgeDir;
use crate::fs::memory::format_memory_for_signal;
use crate::fs::task_state::read_task_state_if_exists;
use crate::handoff::git_handoff::GitHistory;
use crate::handoff::schema::ParsedHandoff;
use crate::models::session::Session;
use crate::models::stage::Stage;
use crate::models::worktree::Worktree;
use crate::skills::SkillIndex;

use super::cache::SignalMetrics;
use super::format::{format_signal_content, format_signal_with_metrics};
use super::types::{DependencyStatus, EmbeddedContext, SandboxSummary};

/// Default maximum number of skill recommendations to include in signals
pub const DEFAULT_MAX_SKILL_RECOMMENDATIONS: usize = 5;

pub fn generate_signal(
    session: &Session,
    stage: &Stage,
    worktree: &Worktree,
    dependencies_status: &[DependencyStatus],
    handoff_file: Option<&str>,
    git_history: Option<&GitHistory>,
    work_dir: &Path,
) -> Result<PathBuf> {
    generate_signal_with_skills(
        session,
        stage,
        worktree,
        dependencies_status,
        handoff_file,
        git_history,
        work_dir,
        None, // No skill index - backward compatible
    )
}

/// Generate a signal file with optional skill recommendations
#[allow(clippy::too_many_arguments)]
pub fn generate_signal_with_skills(
    session: &Session,
    stage: &Stage,
    worktree: &Worktree,
    dependencies_status: &[DependencyStatus],
    handoff_file: Option<&str>,
    git_history: Option<&GitHistory>,
    work_dir: &Path,
    skill_index: Option<&SkillIndex>,
) -> Result<PathBuf> {
    let signals_dir = work_dir.join("signals");

    if !signals_dir.exists() {
        fs::create_dir_all(&signals_dir).context("Failed to create signals directory")?;
    }

    // Build embedded context by reading files, including task state and session memory for recitation
    let mut embedded_context =
        build_embedded_context_with_session(work_dir, handoff_file, &stage.id, Some(&session.id));

    // Add skill recommendations if skill index is available
    if let Some(index) = skill_index {
        let text_to_match = build_skill_match_text(stage);
        embedded_context.skill_recommendations =
            index.match_skills(&text_to_match, DEFAULT_MAX_SKILL_RECOMMENDATIONS);
    }

    // Populate context budget from stage (or use default)
    embedded_context.context_budget = stage
        .context_budget
        .map(|b| b as f32)
        .or(Some(crate::models::constants::DEFAULT_CONTEXT_BUDGET));

    // Populate current context usage from session
    let usage_pct = if session.context_limit > 0 {
        (session.context_tokens as f32 / session.context_limit as f32) * 100.0
    } else {
        0.0
    };
    embedded_context.context_usage = Some(usage_pct);

    // Populate sandbox summary from stage config
    embedded_context.sandbox_summary = Some(build_sandbox_summary(stage));

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

/// Build text for skill matching from stage metadata
fn build_skill_match_text(stage: &Stage) -> String {
    let mut text = stage.name.clone();
    if let Some(desc) = &stage.description {
        text.push(' ');
        text.push_str(desc);
    }
    // Include acceptance criteria for better matching
    for criterion in &stage.acceptance {
        text.push(' ');
        text.push_str(criterion);
    }
    text
}

/// Build embedded context with optional session ID for memory recitation
pub(super) fn build_embedded_context_with_session(
    work_dir: &Path,
    handoff_file: Option<&str>,
    stage_id: &str,
    session_id: Option<&str>,
) -> EmbeddedContext {
    build_embedded_context_with_stage_and_session(
        work_dir,
        handoff_file,
        Some(stage_id),
        session_id,
    )
}

/// Build embedded context with optional stage-specific task state (no session memory)
pub fn build_embedded_context_with_stage(
    work_dir: &Path,
    handoff_file: Option<&str>,
    stage_id: Option<&str>,
) -> EmbeddedContext {
    build_embedded_context_with_stage_and_session(work_dir, handoff_file, stage_id, None)
}

/// Build embedded context with both stage and session info for full recitation
pub fn build_embedded_context_with_stage_and_session(
    work_dir: &Path,
    handoff_file: Option<&str>,
    stage_id: Option<&str>,
    session_id: Option<&str>,
) -> EmbeddedContext {
    let mut context = EmbeddedContext::default();

    // Read handoff content if specified
    if let Some(handoff_name) = handoff_file {
        let handoff_path = work_dir.join("handoffs").join(format!("{handoff_name}.md"));
        if handoff_path.exists() {
            if let Ok(content) = fs::read_to_string(&handoff_path) {
                // Try to parse as V2, fall back to V1
                match ParsedHandoff::parse(&content) {
                    ParsedHandoff::V2(handoff) => {
                        context.parsed_handoff = Some(*handoff);
                        // Still store full content for human-readable sections
                        context.handoff_content = Some(content);
                    }
                    ParsedHandoff::V1Fallback(_) => {
                        // V1 format: just store the raw content
                        context.handoff_content = Some(content);
                    }
                }
            }
        }
    }

    // Read plan overview from config.toml and the plan file
    context.plan_overview = read_plan_overview(work_dir);

    // Check if knowledge directory has meaningful content
    let project_root = work_dir.parent().unwrap_or(work_dir);
    let knowledge = KnowledgeDir::new(project_root);
    context.knowledge_has_content = knowledge.has_content();

    // Read task state if stage_id is provided
    if let Some(stage_id) = stage_id {
        if let Ok(Some(task_state)) = read_task_state_if_exists(work_dir, stage_id) {
            context.task_state = Some(task_state);
        }
    }

    // Read recent memory entries for recitation (Manus pattern - last 10 entries)
    // This keeps important session context in the attention window
    if let Some(sid) = session_id {
        context.memory_content = format_memory_for_signal(work_dir, sid, 10);
    }

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

/// Generate a signal file with metrics about section sizes
///
/// Returns both the signal path and metrics about the signal's structure.
/// Use this for debugging KV-cache efficiency and token usage.
pub fn generate_signal_with_metrics(
    session: &Session,
    stage: &Stage,
    worktree: &Worktree,
    dependencies_status: &[DependencyStatus],
    handoff_file: Option<&str>,
    git_history: Option<&GitHistory>,
    work_dir: &Path,
) -> Result<(PathBuf, SignalMetrics)> {
    let signals_dir = work_dir.join("signals");

    if !signals_dir.exists() {
        fs::create_dir_all(&signals_dir).context("Failed to create signals directory")?;
    }

    // Build embedded context by reading files, including task state and session memory for recitation
    let mut embedded_context =
        build_embedded_context_with_session(work_dir, handoff_file, &stage.id, Some(&session.id));

    // Populate context budget from stage (or use default)
    embedded_context.context_budget = stage
        .context_budget
        .map(|b| b as f32)
        .or(Some(crate::models::constants::DEFAULT_CONTEXT_BUDGET));

    // Populate current context usage from session
    let usage_pct = if session.context_limit > 0 {
        (session.context_tokens as f32 / session.context_limit as f32) * 100.0
    } else {
        0.0
    };
    embedded_context.context_usage = Some(usage_pct);

    // Populate sandbox summary from stage config
    embedded_context.sandbox_summary = Some(build_sandbox_summary(stage));

    let signal_path = signals_dir.join(format!("{}.md", session.id));
    let formatted = format_signal_with_metrics(
        session,
        stage,
        worktree,
        dependencies_status,
        handoff_file,
        git_history,
        &embedded_context,
    );

    fs::write(&signal_path, &formatted.content)
        .with_context(|| format!("Failed to write signal file: {}", signal_path.display()))?;

    Ok((signal_path, formatted.metrics))
}

/// Build sandbox summary from stage configuration
fn build_sandbox_summary(stage: &Stage) -> SandboxSummary {
    // For now, use stage.sandbox directly
    // Later this will use the sandbox::merge_config function to merge plan-level defaults
    SandboxSummary {
        enabled: stage.sandbox.enabled.unwrap_or(true),
        deny_read: stage
            .sandbox
            .filesystem
            .as_ref()
            .map(|f| f.deny_read.clone())
            .unwrap_or_default(),
        deny_write: stage
            .sandbox
            .filesystem
            .as_ref()
            .map(|f| f.deny_write.clone())
            .unwrap_or_default(),
        allow_write: stage
            .sandbox
            .filesystem
            .as_ref()
            .map(|f| f.allow_write.clone())
            .unwrap_or_default(),
        allowed_domains: stage
            .sandbox
            .network
            .as_ref()
            .map(|n| {
                let mut domains = n.allowed_domains.clone();
                domains.extend(n.additional_domains.clone());
                domains
            })
            .unwrap_or_default(),
        excluded_commands: stage.sandbox.excluded_commands.clone(),
    }
}
