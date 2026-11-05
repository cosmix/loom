mod context;
mod enrichment;
mod findings;
mod product_sections;

pub(super) use context::build_embedded_context_for_stage;
pub use context::build_embedded_context_with_stage;
#[cfg(test)]
pub(super) use context::{extract_plan_overview, extract_plan_overview_from};
use enrichment::{build_cross_stage_summary, wants_stage_enrichment};
#[cfg(test)]
pub(super) use findings::build_dependency_findings;
pub(super) use findings::build_dependency_findings_with_changes;
pub(super) use findings::DEPENDENCY_FINDINGS_HEADING;
pub(super) use product_sections::append_product_sections;
// Read only by tests (this crate's and the dependency harness), so the lib
// build would flag an unconditional re-export as unused.
#[cfg(test)]
pub(super) use findings::MAX_DEPENDENCY_FINDINGS_BYTES;

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::fs::work_dir::resolve_context_ceiling_tokens;
use crate::handoff::git_handoff::GitHistory;
use crate::language::DetectedLanguage;
use crate::models::session::Session;
use crate::models::stage::{Stage, StageType};
use crate::models::worktree::Worktree;
use crate::plan::schema::CodeReviewConfig;
use crate::sandbox::PathEscapeAttempt;
use crate::skills::{SkillIndex, SkillMatch};

use super::cache::SignalMetrics;
use super::format::{format_signal_with_metrics, FormattedSignal};
use super::types::{DependencyStatus, EmbeddedContext, SandboxSummary};

/// Default maximum number of skill recommendations to include in signals
pub const DEFAULT_MAX_SKILL_RECOMMENDATIONS: usize = crate::skills::recommend::MAX_RECOMMENDATIONS;

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
        None,
        &[],
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
    let (formatted, context) = assemble_signal(
        session,
        stage,
        worktree,
        dependencies_status,
        handoff_file,
        git_history,
        work_dir,
        skill_index,
        detected_languages,
    );
    super::helpers::publish_signal(&session.id, &formatted.content, work_dir, stage, &context)
}

/// Render the "## Review Dimensions" checklist for an integration-verify signal,
/// framed as mandatory when `require_all` is set and advisory otherwise. Returns
/// `None` when no dimensions are configured.
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

/// Resolve `stage.skills` against the loaded index and put them first,
/// ahead of the keyword/file-type recommendations `recommend::for_files`
/// produces — a plan-declared skill is a requirement, not a guess, so it
/// leads the list `format_skill_recommendations` renders. Marked with
/// `"declared-for-stage"` so the formatter can give it directive wording
/// distinct from a file-type detection. A name the index does not resolve
/// (already reported by `check_declared_skills` at `loom plan verify` time)
/// is silently skipped here rather than surfaced twice.
pub(super) fn declared_and_recommended_skills(
    index: &SkillIndex,
    stage: &Stage,
    worktree: &Worktree,
    detected_languages: &[DetectedLanguage],
) -> Vec<SkillMatch> {
    let declared_names: std::collections::HashSet<&str> =
        stage.skills.iter().map(String::as_str).collect();

    let mut skills: Vec<SkillMatch> = stage
        .skills
        .iter()
        .filter_map(|name| index.get_by_name(name))
        .map(|metadata| {
            SkillMatch::new(
                metadata.name.clone(),
                metadata.description.clone(),
                10.0,
                vec!["declared-for-stage".to_string()],
            )
        })
        .collect();

    skills.extend(
        crate::skills::recommend::for_files(
            index,
            &build_skill_match_text(stage),
            &worktree.path,
            &stage.files,
            detected_languages,
        )
        .into_iter()
        .filter(|recommended| !declared_names.contains(recommended.name.as_str())),
    );

    skills
}

/// Build text for skill matching from stage metadata
fn build_skill_match_text(stage: &Stage) -> String {
    let mut text = stage.name.clone();
    if let Some(desc) = &stage.description {
        text.push(' ');
        text.push_str(desc);
    }
    for criterion in &stage.acceptance {
        text.push(' ');
        text.push_str(criterion.command());
    }
    text
}

/// Generate a signal file with metrics about section sizes, for debugging
/// KV-cache efficiency and token usage.
pub fn generate_signal_with_metrics(
    session: &Session,
    stage: &Stage,
    worktree: &Worktree,
    dependencies_status: &[DependencyStatus],
    handoff_file: Option<&str>,
    git_history: Option<&GitHistory>,
    work_dir: &Path,
) -> Result<(PathBuf, SignalMetrics)> {
    let (formatted, context) = assemble_signal(
        session,
        stage,
        worktree,
        dependencies_status,
        handoff_file,
        git_history,
        work_dir,
        None,
        &[],
    );
    let signal_path =
        super::helpers::publish_signal(&session.id, &formatted.content, work_dir, stage, &context)?;

    Ok((signal_path, formatted.metrics))
}

#[allow(clippy::too_many_arguments)]
fn assemble_signal(
    session: &Session,
    stage: &Stage,
    worktree: &Worktree,
    dependencies_status: &[DependencyStatus],
    handoff_file: Option<&str>,
    git_history: Option<&GitHistory>,
    work_dir: &Path,
    skill_index: Option<&SkillIndex>,
    detected_languages: &[DetectedLanguage],
) -> (FormattedSignal, EmbeddedContext) {
    let mut context = build_signal_context(session, stage, work_dir, handoff_file);
    if let Some(index) = skill_index {
        context.skill_recommendations =
            declared_and_recommended_skills(index, stage, worktree, detected_languages);
    }
    let mut formatted = format_signal_with_metrics(
        session,
        stage,
        worktree,
        dependencies_status,
        handoff_file,
        git_history,
        &context,
    );
    let original_len = formatted.content.len();
    super::v2_section::append_v2_section(&mut formatted.content, stage, work_dir);
    product_sections::append_product_sections(&mut formatted.content, stage, work_dir);
    append_stage_feedback(&mut formatted.content, stage, work_dir);
    // Stage-specific suffixes belong to the dynamic stage content bucket.
    formatted.metrics.dynamic_bytes += formatted.content.len() - original_len;
    formatted.metrics.signal_size_bytes = formatted.content.len();
    formatted.metrics.estimated_tokens = formatted.content.len() / 4;
    (formatted, context)
}

/// Build a stage signal's context. One dependency snapshot feeds source boosts,
/// dependency findings and cross-stage enrichment, so the three sections cannot
/// observe different dependency states during one build.
fn build_signal_context(
    session: &Session,
    stage: &Stage,
    work_dir: &Path,
    handoff_file: Option<&str>,
) -> EmbeddedContext {
    let mut embedded_context = build_embedded_context_for_stage(work_dir, handoff_file, &stage.id);

    // Full three-tier resolution, not the stage's own value alone:
    // the signal must quote the SAME ceiling the hook governs against and the
    // daemon backstops on, `[context] ceiling_tokens` tier included, or the
    // agent reads itself as nearer its limit than it is and hands off early.
    embedded_context.context_ceiling_tokens = Some(resolve_context_ceiling_tokens(
        work_dir,
        stage.context_ceiling_tokens,
    ));
    embedded_context.context_tokens = Some(session.context_tokens);

    embedded_context.sandbox_summary = Some(build_sandbox_summary(stage, work_dir));

    // Ultracode license and implementer lanes gate the semi-stable section.
    embedded_context.ultracode = stage.ultracode;
    embedded_context.implementers = stage.implementers.clone();

    // Only set when the plan made a deliberate choice: the orchestrator always
    // measures against `effective_subagent_timeout_secs()`, but the signal tells
    // the agent about it only when there's a value worth acting on.
    embedded_context.subagent_timeout_secs = stage.subagent_timeout_secs;

    let changes = super::retrieval::dependency_changes(work_dir, stage);
    if wants_stage_enrichment(stage) {
        embedded_context.cross_stage_summary = build_cross_stage_summary(stage, &changes);
    }

    embedded_context.context_pack =
        super::retrieval::retrieve_stage_pack_with_changes(work_dir, stage, &changes);
    // After retrieval: relevance reads the brief's paths. Every stage type; the
    // selector decides. Rebuilt from the journals at every generation, so a
    // continuation picks up entries relayed after the first signal.
    embedded_context.dependency_findings = build_dependency_findings_with_changes(
        work_dir,
        stage,
        embedded_context.context_pack.as_ref(),
        &changes,
    );
    embedded_context.knowledge_tree_empty = super::helpers::knowledge_tree_is_empty(work_dir);
    embedded_context
}

/// Project the display summary from the effective launcher configuration: the
/// stage's overrides merged over the plan, plus the licensed codex lane's grants.
fn build_sandbox_summary(stage: &Stage, work_dir: &Path) -> SandboxSummary {
    let merged = crate::sandbox::stage_sandbox(work_dir, stage);
    let mut expanded = merged.clone();
    crate::sandbox::expand_paths(&mut expanded);
    let missing_allow_write = crate::sandbox::missing_grant_paths(
        &expanded.filesystem.allow_write,
        dirs::home_dir().as_deref(),
    );
    let mut allow_write: Vec<String> = merged
        .filesystem
        .allow_write
        .iter()
        // This grant is for the privileged knowledge command, not direct edits.
        .filter(|path| {
            let path = path.trim();
            path != "doc/loom/knowledge" && !path.starts_with("doc/loom/knowledge/")
        })
        // Settings emission drops these, so they grant nothing.
        .filter(|path| crate::sandbox::detect_path_escape(path.trim()) == PathEscapeAttempt::Safe)
        .cloned()
        .collect();
    let mut allowed_domains = merged.network.allowed_domains.clone();
    allowed_domains.extend(merged.network.additional_domains.clone());
    if merged.implementers.includes_codex() {
        extend_unique(&mut allowed_domains, &crate::codex::CODEX_SANDBOX_DOMAINS);
        extend_unique(&mut allow_write, &crate::codex::CODEX_SANDBOX_WRITE_PATHS);
    }

    SandboxSummary {
        enabled: merged.enabled,
        deny_read: merged.filesystem.deny_read.clone(),
        deny_write: merged.filesystem.deny_write.clone(),
        allow_write,
        missing_allow_write,
        allowed_domains,
        unix_sockets_allowed: crate::sandbox::grants_all_unix_sockets(&merged),
        excluded_commands: merged.excluded_commands.clone(),
    }
}

fn extend_unique(into: &mut Vec<String>, values: &[&str]) {
    for value in values {
        if !into.iter().any(|existing| existing == value) {
            into.push((*value).to_string());
        }
    }
}

fn append_stage_feedback(content: &mut String, stage: &Stage, work_dir: &Path) {
    // A resolver's committed merge failed verification: the section is not
    // gated on disputes, since the stage may never have filed one.
    super::section_formatters::append_merge_handback_section(content, stage);

    // Adjudicator feedback (disputed stages only), appended last so it sits
    // where the agent's recitation attention is highest.
    if has_disputes(stage) {
        if let Ok(Some(text)) =
            crate::orchestrator::adjudication::feedback::read_feedback(work_dir, &stage.id)
        {
            content.push_str("\n## Adjudicator Feedback (from your prior dispute)\n\n");
            content.push_str(&text);
            super::helpers::ensure_trailing_newline(content);
        }
    }

    // Surface the stage's code-review dimensions to integration-verify agents.
    if matches!(stage.stage_type, StageType::IntegrationVerify) {
        if let Some(section) = stage
            .code_review
            .as_ref()
            .and_then(render_review_dimensions)
        {
            content.push_str(&section);
            super::helpers::ensure_trailing_newline(content);
        }
    }
}

/// Whether a dispute of any kind was ever filed for `stage`, so adjudicator
/// feedback may be waiting for it.
fn has_disputes(stage: &Stage) -> bool {
    [
        stage.dispute_count,
        stage.tally.finding_disputes,
        stage.tally.contract_disputes,
        stage.tally.integrity_disputes,
        stage.tally.impact_disputes,
    ]
    .iter()
    .any(|&count| count > 0)
}

#[cfg(test)]
#[path = "generate_declared_skills_tests.rs"]
mod declared_skills_tests;

#[cfg(test)]
#[path = "generate_feedback_tests.rs"]
mod feedback_tests;

#[cfg(test)]
#[path = "generate_missing_allow_write_tests.rs"]
mod missing_allow_write_tests;

#[cfg(test)]
#[path = "generate_dependency_findings_tests.rs"]
mod dependency_findings_tests;
