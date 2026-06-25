use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

use crate::fs::knowledge::KnowledgeDir;
use crate::fs::memory::format_memory_for_signal;
use crate::handoff::git_handoff::GitHistory;
use crate::handoff::schema::ParsedHandoff;
use crate::language::{detect_languages_from_files, DetectedLanguage};
use crate::models::session::Session;
use crate::models::stage::{Stage, StageType};
use crate::models::worktree::Worktree;
use crate::plan::schema::CodeReviewConfig;
use crate::skills::{SkillIndex, SkillMatch, SkillMetadata};
use crate::verify::transitions::load_stage;

use super::cache::SignalMetrics;
use super::format::{format_signal_content, format_signal_with_metrics};
use super::types::{DependencyStatus, EmbeddedContext, SandboxSummary};

/// Default maximum number of skill recommendations to include in signals
pub const DEFAULT_MAX_SKILL_RECOMMENDATIONS: usize = 5;

/// Score assigned to skills injected via project language detection.
/// Higher than trigger-based scores (1.0 word, 2.0 phrase) to ensure
/// language-detected skills appear prominently in recommendations.
const LANGUAGE_DETECTION_SCORE: f32 = 10.0;

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
        &[],  // No detected languages - backward compatible
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
    detected_languages: &[DetectedLanguage],
) -> Result<PathBuf> {
    // Build embedded context with shared setup logic
    let mut embedded_context = build_signal_context(session, stage, work_dir, handoff_file);

    // Add skill recommendations if skill index is available
    if let Some(index) = skill_index {
        let text_to_match = build_skill_match_text(stage);
        embedded_context.skill_recommendations =
            index.match_skills(&text_to_match, DEFAULT_MAX_SKILL_RECOMMENDATIONS);

        // Inject language skills for the files THIS stage will edit, so the agent
        // loads the matching `loom-<lang>` skill before writing code. Detection is
        // file-scoped (so a monorepo's frontend stage gets loom-typescript while a
        // backend stage gets loom-rust); we fall back to the project-wide languages
        // only when the stage declares no files.
        let stage_languages = detect_languages_from_files(&stage.files);
        let languages: &[DetectedLanguage] = if stage_languages.is_empty() {
            detected_languages
        } else {
            &stage_languages
        };

        for lang in languages {
            if let Some(metadata) = resolve_language_skill(index, lang.skill_name()) {
                // Only add if not already in recommendations (dedup by name)
                if !embedded_context
                    .skill_recommendations
                    .iter()
                    .any(|s| s.name == metadata.name)
                {
                    embedded_context.skill_recommendations.push(SkillMatch::new(
                        metadata.name.clone(),
                        metadata.description.clone(),
                        LANGUAGE_DETECTION_SCORE,
                        vec!["project-language".to_string()],
                    ));
                }
            }
        }
    }

    let mut content = format_signal_content(
        session,
        stage,
        worktree,
        dependencies_status,
        handoff_file,
        git_history,
        &embedded_context,
    );

    // Inject adjudicator feedback (if any) for stages that have been
    // disputed. Appended after the formatted sections so it sits near
    // the end where the agent's recitation attention is highest.
    if stage.dispute_count > 0 {
        if let Ok(Some(text)) =
            crate::orchestrator::adjudication::feedback::read_feedback(work_dir, &stage.id)
        {
            content.push_str("\n## Adjudicator Feedback (from your prior dispute)\n\n");
            content.push_str(&text);
            if !content.ends_with('\n') {
                content.push('\n');
            }
        }
    }

    // Surface the plan's structured code-review dimensions to integration-verify
    // agents as an actionable checklist. `code_review` lives only on the plan's
    // `StageDefinition` (not the runtime `Stage`), so we read it back from the
    // plan here — the same source the after-stage checks use at completion time.
    // Gated to integration-verify so the plan is only re-parsed for the (rare)
    // review-gate spawns, never on every stage spawn.
    if matches!(stage.stage_type, StageType::IntegrationVerify) {
        if let Some(section) = load_code_review_for_stage(work_dir, &stage.id)
            .and_then(|c| render_review_dimensions(&c))
        {
            content.push_str(&section);
            if !content.ends_with('\n') {
                content.push('\n');
            }
        }
    }

    super::helpers::write_signal_file(&session.id, &content, work_dir)
}

/// Load the `code_review` configuration for `stage_id` from the active plan.
///
/// The runtime [`Stage`] model does not carry `code_review` — it lives only on
/// the plan's `StageDefinition`. We read it back via the canonical plan loader
/// ([`load_stage_definition_from_plan`](crate::plan::load_stage_definition_from_plan)),
/// the same helper after-stage checks use at completion time. Returns `None` on
/// any failure (no plan configured, parse error, stage not found, or no
/// `code_review` set); a missing review config is never fatal to signal generation.
fn load_code_review_for_stage(work_dir: &Path, stage_id: &str) -> Option<CodeReviewConfig> {
    crate::plan::load_stage_definition_from_plan(stage_id, work_dir)
        .ok()
        .flatten()
        .and_then(|s| s.code_review)
}

/// Render the "## Review Dimensions" section for an integration-verify signal.
///
/// Surfaces the plan's `code_review` configuration to the agent as an actionable
/// checklist — one checkbox per dimension. `require_all` controls the framing:
/// when `true` every dimension MUST be explicitly addressed before completion;
/// when `false` the dimensions are advisory ("where applicable").
///
/// Returns `None` when no dimensions are configured, so the caller appends
/// nothing rather than an empty heading.
pub(super) fn render_review_dimensions(config: &CodeReviewConfig) -> Option<String> {
    if config.dimensions.is_empty() {
        return None;
    }

    let mut section = String::from("\n## Review Dimensions\n\n");
    if config.require_all {
        section.push_str(
            "Your review MUST explicitly address **every** dimension below before completing \
             this stage (`require_all`). State your findings for each:\n\n",
        );
    } else {
        section.push_str("Address the following review dimensions where applicable:\n\n");
    }
    for dimension in &config.dimensions {
        section.push_str(&format!("- [ ] **{dimension}**\n"));
    }
    Some(section)
}

/// Resolve a detected language to its skill metadata in the index.
///
/// Installed skills follow the `loom-<topic>` naming convention (`loom-rust`,
/// `loom-typescript`, ...), but [`DetectedLanguage::skill_name`](crate::language::DetectedLanguage::skill_name)
/// returns the bare topic (`rust`). We try the prefixed name first, then the bare
/// name, so the lookup works whether or not skills carry the `loom-` prefix.
///
/// This guards against a silent failure mode: a plain `get_by_name("rust")` never
/// matches a skill named `loom-rust`, so language skills would never be injected.
fn resolve_language_skill<'a>(index: &'a SkillIndex, base: &str) -> Option<&'a SkillMetadata> {
    index
        .get_by_name(&format!("loom-{base}"))
        .or_else(|| index.get_by_name(base))
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
        text.push_str(criterion.command());
    }
    text
}

/// Build embedded context for a stage's memory recitation
pub(super) fn build_embedded_context_for_stage(
    work_dir: &Path,
    handoff_file: Option<&str>,
    stage_id: &str,
) -> EmbeddedContext {
    build_embedded_context_with_stage_and_session(work_dir, handoff_file, Some(stage_id))
}

/// Build embedded context with optional stage-specific task state (no session memory)
pub fn build_embedded_context_with_stage(
    work_dir: &Path,
    handoff_file: Option<&str>,
    stage_id: Option<&str>,
) -> EmbeddedContext {
    build_embedded_context_with_stage_and_session(work_dir, handoff_file, stage_id)
}

/// Build embedded context with both stage and session info for full recitation
pub fn build_embedded_context_with_stage_and_session(
    work_dir: &Path,
    handoff_file: Option<&str>,
    stage_id: Option<&str>,
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

    // Read recent memory entries for recitation (Manus pattern - last 10 entries)
    // This keeps important stage context in the attention window
    if let Some(sid) = stage_id {
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
    // Build embedded context with shared setup logic
    let embedded_context = build_signal_context(session, stage, work_dir, handoff_file);

    let formatted = format_signal_with_metrics(
        session,
        stage,
        worktree,
        dependencies_status,
        handoff_file,
        git_history,
        &embedded_context,
    );

    let signal_path = super::helpers::write_signal_file(&session.id, &formatted.content, work_dir)?;

    Ok((signal_path, formatted.metrics))
}

/// Build cross-stage change summary for integration-verify stages.
///
/// For each completed dependency, aggregates file assignments and stage metadata
/// to give integration-verify agents a bird's eye view of all changes.
fn build_cross_stage_summary(work_dir: &Path, stage: &Stage) -> Option<String> {
    if !matches!(
        stage.stage_type,
        StageType::IntegrationVerify | StageType::KnowledgeDistill
    ) {
        return None;
    }

    if stage.dependencies.is_empty() {
        return None;
    }

    let mut summary = String::from("## Cross-Stage Changes\n\n");
    let mut has_content = false;
    let mut all_files: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();

    for dep_id in &stage.dependencies {
        match load_stage(dep_id, work_dir) {
            Ok(dep_stage) => {
                has_content = true;
                summary.push_str(&format!(
                    "### Stage: {} ({})\n",
                    dep_stage.name,
                    format_stage_status(&dep_stage.status)
                ));
                summary.push_str(&format!("Branch: loom/{dep_id}\n"));

                if !dep_stage.files.is_empty() {
                    summary.push_str("Files:\n");
                    for file in &dep_stage.files {
                        summary.push_str(&format!("- {file}\n"));
                        all_files
                            .entry(file.clone())
                            .or_default()
                            .push(dep_id.clone());
                    }
                }
                summary.push('\n');
            }
            Err(_) => {
                // Stage file not found or unreadable - skip gracefully
            }
        }
    }

    if !has_content {
        return None;
    }

    // Identify files touched by multiple stages
    let multi_stage_files: Vec<_> = all_files
        .iter()
        .filter(|(_, stages)| stages.len() > 1)
        .collect();

    let new_file_count: usize = all_files.values().filter(|s| s.len() == 1).count();

    if !multi_stage_files.is_empty() || new_file_count > 0 {
        summary.push_str("### Potential Concerns\n");
        for (file, stages) in &multi_stage_files {
            summary.push_str(&format!(
                "- `{}` modified by {} stages — verify no conflicts\n",
                file,
                stages.len()
            ));
        }
        if new_file_count > 0 {
            summary.push_str(&format!(
                "- {} new file(s) added — verify all are wired\n",
                new_file_count
            ));
        }
        summary.push('\n');
    }

    Some(summary)
}

/// Format a stage status for display
fn format_stage_status(status: &crate::models::stage::StageStatus) -> &'static str {
    use crate::models::stage::StageStatus;
    match status {
        StageStatus::Completed => "completed",
        StageStatus::Executing => "executing",
        StageStatus::Queued => "queued",
        StageStatus::WaitingForDeps => "waiting",
        StageStatus::Blocked => "blocked",
        StageStatus::NeedsHandoff => "needs-handoff",
        StageStatus::WaitingForInput => "waiting-for-input",
        StageStatus::MergeConflict => "merge-conflict",
        StageStatus::Skipped => "skipped",
        StageStatus::CompletedWithFailures => "completed-with-failures",
        StageStatus::MergeBlocked => "merge-blocked",
        StageStatus::NeedsHumanReview => "needs-human-review",
        StageStatus::NeedsAdjudication => "needs-adjudication",
    }
}

/// Build wiring checklist from stage memories for integration-verify.
///
/// Reads memory entries from all completed stages and extracts
/// wiring-related notes into an actionable checklist.
fn build_wiring_checklist(work_dir: &Path, stage: &Stage) -> Option<String> {
    if !matches!(
        stage.stage_type,
        StageType::IntegrationVerify | StageType::KnowledgeDistill
    ) {
        return None;
    }

    if stage.dependencies.is_empty() {
        return None;
    }

    // Keywords indicating wiring-relevant notes
    let wiring_keywords = [
        "needs", "wire", "wiring", "register", "mount", "import", "add to", "connect",
    ];

    let mut checklist = String::from("## Downstream Wiring Checklist\n\n");
    let mut has_items = false;

    for dep_id in &stage.dependencies {
        let memory_path = work_dir.join("memory").join(format!("{dep_id}.md"));

        let content = match fs::read_to_string(&memory_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // Load stage name for display
        let stage_name = load_stage(dep_id, work_dir)
            .map(|s| s.name)
            .unwrap_or_else(|_| dep_id.clone());

        let mut stage_items: Vec<String> = Vec::new();

        for line in content.lines() {
            let lower = line.to_lowercase();
            if wiring_keywords.iter().any(|kw| lower.contains(kw)) {
                // Strip common markdown prefixes for cleaner display
                let stripped = line
                    .trim_start_matches('-')
                    .trim_start_matches('*')
                    .trim_start_matches('#')
                    .trim();
                if !stripped.is_empty() {
                    stage_items.push(stripped.to_string());
                }
            }
        }

        if !stage_items.is_empty() {
            has_items = true;
            checklist.push_str(&format!("From stage '{stage_name}':\n"));
            for item in stage_items {
                checklist.push_str(&format!("- [ ] {item}\n"));
            }
            checklist.push('\n');
        }
    }

    if !has_items {
        return None;
    }

    Some(checklist)
}

/// Build signal context with all shared setup logic
///
/// This consolidates the context building, budget, usage, and sandbox setup
/// that was duplicated between `generate_signal_with_skills` and `generate_signal_with_metrics`.
fn build_signal_context(
    session: &Session,
    stage: &Stage,
    work_dir: &Path,
    handoff_file: Option<&str>,
) -> EmbeddedContext {
    // Build embedded context by reading files, including task state and stage memory for recitation
    let mut embedded_context = build_embedded_context_for_stage(work_dir, handoff_file, &stage.id);

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

    // Propagate the ultracode license so the semi-stable section can gate on it
    embedded_context.ultracode = stage.ultracode;

    // Build integration-verify and knowledge-distill enrichments
    if matches!(
        stage.stage_type,
        StageType::IntegrationVerify | StageType::KnowledgeDistill
    ) {
        embedded_context.cross_stage_summary = build_cross_stage_summary(work_dir, stage);
        embedded_context.wiring_checklist = build_wiring_checklist(work_dir, stage);
    }

    embedded_context
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

#[cfg(test)]
mod resolve_skill_tests {
    use super::resolve_language_skill;
    use crate::language::DetectedLanguage;
    use crate::skills::SkillIndex;
    use std::fs;
    use std::io::Write;
    use tempfile::TempDir;

    /// Build a skill index from a temp dir containing one skill named `name`.
    fn index_with_skill(name: &str) -> (TempDir, SkillIndex) {
        let temp = TempDir::new().unwrap();
        let skill_dir = temp.path().join(name);
        fs::create_dir_all(&skill_dir).unwrap();
        let mut f = fs::File::create(skill_dir.join("SKILL.md")).unwrap();
        writeln!(f, "---").unwrap();
        writeln!(f, "name: {name}").unwrap();
        writeln!(f, "description: Test skill").unwrap();
        writeln!(f, "---").unwrap();
        let index = SkillIndex::load_from_directory(temp.path()).unwrap();
        (temp, index)
    }

    #[test]
    fn resolves_loom_prefixed_skill_from_bare_language_name() {
        // The real failure mode: skills are installed as `loom-rust` but the
        // language reports the bare name `rust`. The resolver must bridge that.
        let (_temp, index) = index_with_skill("loom-rust");
        let base = DetectedLanguage::Rust.skill_name();
        assert_eq!(base, "rust");
        let resolved = resolve_language_skill(&index, base).expect("loom-rust must resolve");
        assert_eq!(resolved.name, "loom-rust");
    }

    #[test]
    fn resolves_bare_skill_when_unprefixed() {
        let (_temp, index) = index_with_skill("python");
        let resolved = resolve_language_skill(&index, DetectedLanguage::Python.skill_name())
            .expect("bare python must resolve");
        assert_eq!(resolved.name, "python");
    }

    #[test]
    fn returns_none_when_absent() {
        let (_temp, index) = index_with_skill("loom-rust");
        assert!(resolve_language_skill(&index, "golang").is_none());
    }
}
