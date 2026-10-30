//! The `[models]` section of `.loom/work/config.toml`: the project tier of a
//! stage type's model/reasoning-effort policy. See
//! [`resolve_stage_model_effort`] for the full four-tier chain this section
//! sits in.

use std::path::Path;

use crate::models::stage::{StageType, ALLOWED_REASONING_EFFORTS};
use crate::user_config::UserConfig;

use super::allowed::allowed_value;

/// Section name of the project-level stage model policy.
const MODELS_SECTION: &str = "models";

/// The `[models]` section of `.loom/work/config.toml`.
///
/// Every key is optional and the fallback is KEY-level: a present section
/// that omits a key falls through to `~/.loom/config.toml` for that key
/// rather than shadowing it with a built-in the operator never asked for —
/// the same rule `[terminal]`/`[context]` resolve under (see
/// `read_terminal_config`, `read_context_config`).
#[derive(Debug, Clone, Default, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ModelsConfig {
    standard_model: Option<String>,
    standard_effort: Option<String>,
    knowledge_model: Option<String>,
    knowledge_effort: Option<String>,
    knowledge_distill_model: Option<String>,
    knowledge_distill_effort: Option<String>,
    integration_verify_model: Option<String>,
    integration_verify_effort: Option<String>,
    code_reviewer_model: Option<String>,
    code_reviewer_followup_model: Option<String>,
    code_reviewer_effort: Option<String>,
    contract_writer_model: Option<String>,
    contract_writer_effort: Option<String>,
}

impl ModelsConfig {
    /// Parse a `[models]` body, e.g. `standard_model = "sonnet"` - the
    /// filesystem-free seam this module's tests build sections with. Nothing
    /// in a production build reaches it: `read_models_config` deserializes the
    /// section straight out of the file.
    #[cfg(test)]
    fn from_section_toml(body: &str) -> anyhow::Result<Self> {
        toml::from_str(body)
            .map_err(|e| anyhow::anyhow!("failed to parse [models] section body: {e}"))
    }

    /// The validated model this section sets for `stage_type`, if any.
    fn model_for(&self, stage_type: StageType) -> Option<&str> {
        let (value, key) = match stage_type {
            StageType::Standard => (&self.standard_model, "models.standard_model"),
            StageType::Knowledge => (&self.knowledge_model, "models.knowledge_model"),
            StageType::KnowledgeDistill => (
                &self.knowledge_distill_model,
                "models.knowledge_distill_model",
            ),
            StageType::IntegrationVerify => (
                &self.integration_verify_model,
                "models.integration_verify_model",
            ),
        };
        allowed_value(value.as_ref(), crate::claude::CLAUDE_MODELS, key)
    }

    /// The validated reasoning effort this section sets for `stage_type`, if any.
    fn effort_for(&self, stage_type: StageType) -> Option<&str> {
        let (value, key) = match stage_type {
            StageType::Standard => (&self.standard_effort, "models.standard_effort"),
            StageType::Knowledge => (&self.knowledge_effort, "models.knowledge_effort"),
            StageType::KnowledgeDistill => (
                &self.knowledge_distill_effort,
                "models.knowledge_distill_effort",
            ),
            StageType::IntegrationVerify => (
                &self.integration_verify_effort,
                "models.integration_verify_effort",
            ),
        };
        allowed_value(value.as_ref(), ALLOWED_REASONING_EFFORTS, key)
    }
}

impl ModelsConfig {
    /// The validated `loom-code-reviewer` model this section sets, if any.
    fn code_reviewer_model(&self) -> Option<&str> {
        allowed_value(
            self.code_reviewer_model.as_ref(),
            crate::claude::CLAUDE_MODELS,
            "models.code_reviewer_model",
        )
    }

    fn code_reviewer_followup_model(&self) -> Option<&str> {
        allowed_value(
            self.code_reviewer_followup_model.as_ref(),
            crate::claude::CLAUDE_MODELS,
            "models.code_reviewer_followup_model",
        )
    }

    /// The validated `loom-code-reviewer` effort this section sets, if any.
    fn code_reviewer_effort(&self) -> Option<&str> {
        allowed_value(
            self.code_reviewer_effort.as_ref(),
            ALLOWED_REASONING_EFFORTS,
            "models.code_reviewer_effort",
        )
    }
}

impl ModelsConfig {
    /// The validated contract writer model this section sets, if any.
    fn contract_writer_model(&self) -> Option<&str> {
        allowed_value(
            self.contract_writer_model.as_ref(),
            crate::claude::CLAUDE_MODELS,
            "models.contract_writer_model",
        )
    }

    /// The validated contract writer effort this section sets, if any.
    fn contract_writer_effort(&self) -> Option<&str> {
        allowed_value(
            self.contract_writer_effort.as_ref(),
            ALLOWED_REASONING_EFFORTS,
            "models.contract_writer_effort",
        )
    }
}

/// Read `[models]`; a missing, unreadable or malformed section yields an
/// all-`None` config so a broken workspace file falls through to the user tier
/// instead of taking down a spawn (the stance `resolve_context_ceiling_tokens`
/// already takes).
fn read_models_config(work_dir: &Path) -> ModelsConfig {
    super::read_section::<ModelsConfig>(work_dir, MODELS_SECTION)
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// Pure resolution, filesystem-free: the seam this module's tests exercise
/// every tier of the chain through.
fn resolve_from(
    project: &ModelsConfig,
    user: &UserConfig,
    stage_type: StageType,
    stage_model: Option<&str>,
    stage_effort: Option<&str>,
) -> (String, String) {
    let model = stage_model
        .map(str::to_string)
        .or_else(|| project.model_for(stage_type).map(str::to_string))
        .unwrap_or_else(|| user.stage_model(stage_type).to_string());
    let effort = stage_effort
        .map(str::to_string)
        .or_else(|| project.effort_for(stage_type).map(str::to_string))
        .unwrap_or_else(|| user.stage_reasoning_effort(stage_type).to_string());
    (model, effort)
}

/// The (model, reasoning effort) a stage's agent session launches with.
///
/// ONE resolution order, and every reader of a stage's model or effort must
/// use it: the stage's own plan fields -> `.loom/work/config.toml` `[models]`
/// -> `~/.loom/config.toml` `[models]` -> `StageType`'s built-in. Skipping a
/// tier makes the dashboard and the spawn quote different models for one
/// session.
///
/// Takes the stage's own values rather than the `Stage` itself, so `fs/` keeps
/// no dependency on the stage model - the same shape
/// `resolve_context_ceiling_tokens` uses. Call it as
/// `resolve_stage_model_effort(work_dir, stage.stage_type, stage.model.as_deref(), stage.reasoning_effort.as_deref())`.
pub fn resolve_stage_model_effort(
    work_dir: &Path,
    stage_type: StageType,
    stage_model: Option<&str>,
    stage_effort: Option<&str>,
) -> (String, String) {
    let project = read_models_config(work_dir);
    let user = UserConfig::load();
    resolve_from(&project, &user, stage_type, stage_model, stage_effort)
}

/// Pure code-reviewer resolution: project key, else user tier (which already
/// collapses "user set it" and the built-in).
fn resolve_code_reviewer_from(project: &ModelsConfig, user: &UserConfig) -> (String, String) {
    let model = project
        .code_reviewer_model()
        .unwrap_or_else(|| user.code_reviewer_model());
    let effort = project
        .code_reviewer_effort()
        .unwrap_or_else(|| user.code_reviewer_effort());
    (model.to_string(), effort.to_string())
}

/// The (model, reasoning effort) every `loom-code-reviewer` subagent spawn
/// runs with: `.loom/work/config.toml` `[models]` -> `~/.loom/config.toml`
/// `[models]` -> the built-in. Not a stage type, so it has no plan-field tier
/// and does not go through [`resolve_stage_model_effort`]. Each key falls
/// through independently; `work_dir` is `None` outside a workspace.
///
/// Strict, unlike the stage resolver: the caller is the spawn guard, which
/// must fail loudly on an unreadable config rather than spawn a reviewer on a
/// model nobody chose.
pub fn resolve_code_reviewer_model_effort(
    work_dir: Option<&Path>,
) -> anyhow::Result<(String, String)> {
    let project = match work_dir {
        Some(dir) => super::read_section::<ModelsConfig>(dir, MODELS_SECTION)?.unwrap_or_default(),
        None => ModelsConfig::default(),
    };
    Ok(resolve_code_reviewer_from(
        &project,
        &UserConfig::load_strict()?,
    ))
}

/// Follow-up reviewer model and shared effort, each resolved independently.
pub fn resolve_code_reviewer_followup_model_effort(
    work_dir: Option<&Path>,
) -> anyhow::Result<(String, String)> {
    let project = match work_dir {
        Some(dir) => super::read_section::<ModelsConfig>(dir, MODELS_SECTION)?.unwrap_or_default(),
        None => ModelsConfig::default(),
    };
    let user = UserConfig::load_strict()?;
    let model = project
        .code_reviewer_followup_model()
        .unwrap_or_else(|| user.code_reviewer_followup_model());
    let effort = project
        .code_reviewer_effort()
        .unwrap_or_else(|| user.code_reviewer_effort());
    Ok((model.to_string(), effort.to_string()))
}

/// Pure contract-writer resolution: project key, else user tier (which already
/// collapses "user set it" and the built-in).
fn resolve_contract_writer_from(project: &ModelsConfig, user: &UserConfig) -> (String, String) {
    let model = project
        .contract_writer_model()
        .unwrap_or_else(|| user.contract_writer_model());
    let effort = project
        .contract_writer_effort()
        .unwrap_or_else(|| user.contract_writer_effort());
    (model.to_string(), effort.to_string())
}

/// The (model, reasoning effort) a contract writer session launches with:
/// `.loom/work/config.toml` `[models]` -> `~/.loom/config.toml` `[models]` ->
/// the built-in. A plan stage's `model` / `reasoning_effort` do not apply: they
/// choose the implementer orchestrator, and the contract writer is decoupled
/// from it. Each key falls through independently.
///
/// Lenient like [`resolve_stage_model_effort`]: the caller is a spawn, so a
/// malformed project section falls through instead of failing the launch.
pub fn resolve_contract_writer_model_effort(work_dir: &Path) -> (String, String) {
    let project = read_models_config(work_dir);
    let user = UserConfig::load();
    resolve_contract_writer_from(&project, &user)
}

#[cfg(test)]
mod tests;
