mod context;
mod enrichment;

pub(super) use context::build_embedded_context_for_stage;
pub use context::build_embedded_context_with_stage;
#[cfg(test)]
pub(super) use context::{extract_plan_overview, extract_plan_overview_from};
use enrichment::{build_cross_stage_summary, build_wiring_checklist, wants_stage_enrichment};

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::fs::work_dir::resolve_context_ceiling_tokens;
use crate::handoff::git_handoff::GitHistory;
use crate::language::DetectedLanguage;
use crate::models::session::Session;
use crate::models::stage::{Stage, StageType};
use crate::models::worktree::Worktree;
use crate::plan::schema::CodeReviewConfig;
use crate::skills::{SkillIndex, SkillMatch};

use super::cache::SignalMetrics;
use super::format::{format_signal_content, format_signal_with_metrics};
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
    let mut embedded_context = build_signal_context(session, stage, work_dir, handoff_file);

    if let Some(index) = skill_index {
        embedded_context.skill_recommendations =
            declared_and_recommended_skills(index, stage, worktree, detected_languages);
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

    append_stage_feedback(&mut content, stage, work_dir);

    super::helpers::persist_delivery(work_dir, stage, &session.id, &embedded_context);
    super::helpers::write_signal_file(&session.id, &content, work_dir)
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
fn declared_and_recommended_skills(
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

    super::helpers::persist_delivery(work_dir, stage, &session.id, &embedded_context);
    let signal_path = super::helpers::write_signal_file(&session.id, &formatted.content, work_dir)?;

    Ok((signal_path, formatted.metrics))
}

/// Build signal context with all shared setup logic, consolidating context,
/// budget, usage, and sandbox setup duplicated across the two signal generators.
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

    if wants_stage_enrichment(stage) {
        embedded_context.cross_stage_summary = build_cross_stage_summary(work_dir, stage);
        embedded_context.wiring_checklist = build_wiring_checklist(work_dir, stage);
    }

    embedded_context.context_pack = super::helpers::retrieve_stage_pack(work_dir, stage);
    embedded_context.knowledge_tree_empty = super::helpers::knowledge_tree_is_empty(work_dir);
    embedded_context
}

/// Build sandbox summary from stage configuration
fn build_sandbox_summary(stage: &Stage, work_dir: &Path) -> SandboxSummary {
    let allow_write: Vec<String> = stage
        .sandbox
        .filesystem
        .as_ref()
        .map(|f| f.allow_write.clone())
        .unwrap_or_default();
    let missing_allow_write = missing_allow_write_from_merged(work_dir, stage);

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
        allow_write,
        missing_allow_write,
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

/// Compute `missing_allow_write` from the MERGED plan+stage sandbox grants,
/// mirroring the merge the spawn path performs
/// (`orchestrator::core::stage_executor`: `sandbox::merge_config` then
/// `sandbox::expand_paths`) — a plan-level `allow_write` grant (declared in
/// `sandbox.filesystem.allow_write` at the top of the plan, not under a
/// stage) is invisible to `stage.sandbox` alone, so checking only the stage
/// config misses it.
///
/// The displayed `allow_write` list in `build_sandbox_summary` stays
/// stage-only; only this "missing on host" check needs the wider merged set.
/// That set also carries the knowledge write grant
/// (`doc/loom/knowledge/**`, added by `merge_config`'s
/// `apply_knowledge_write_grant`), which stage agents must not see
/// advertised as a plain filesystem path — it never leaks into this list
/// because it is a relative glob, and `missing_grant_paths` only checks
/// absolute or `~/` entries for existence.
fn missing_allow_write_from_merged(work_dir: &Path, stage: &Stage) -> Vec<String> {
    let plan_sandbox = crate::fs::work_dir::read_plan_sandbox(work_dir)
        .ok()
        .flatten()
        .unwrap_or_default();
    let mut merged = crate::sandbox::merge_config(
        &plan_sandbox,
        &stage.sandbox,
        stage.stage_type,
        &stage.implementers,
    );
    crate::sandbox::expand_paths(&mut merged);
    crate::sandbox::missing_grant_paths(&merged.filesystem.allow_write, dirs::home_dir().as_deref())
}

fn append_stage_feedback(content: &mut String, stage: &Stage, work_dir: &Path) {
    // Adjudicator feedback (disputed stages only), appended last so it sits
    // where the agent's recitation attention is highest.
    if stage.dispute_count > 0 {
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

#[cfg(test)]
#[path = "generate_declared_skills_tests.rs"]
mod declared_skills_tests;

#[cfg(test)]
#[path = "generate_missing_allow_write_tests.rs"]
mod missing_allow_write_tests;
