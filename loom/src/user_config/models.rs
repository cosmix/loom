//! The `[models]` section of `~/.loom/config.toml`: the model and reasoning
//! effort each stage type's main-agent session runs on absent a plan field or
//! a project-tier override. Takes the same shape as `pressure.rs` for
//! `[pressure]`; split out of `mod.rs` to keep that file under Rule 17's
//! 400-line limit.

use anyhow::Result;
use toml_edit::DocumentMut;

use crate::models::stage::StageType;

use super::{parse, ConfigValue, KeySpec, Origin, UserConfig};

/// The `[models]` section as the file set it. Twelve keys: one model/effort pair
/// per [`StageType`] plus the `loom-code-reviewer` subagent's pair and the
/// contract writer session's pair.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ModelsSection {
    standard_model: Option<String>,
    standard_effort: Option<String>,
    knowledge_model: Option<String>,
    knowledge_effort: Option<String>,
    knowledge_distill_model: Option<String>,
    knowledge_distill_effort: Option<String>,
    integration_verify_model: Option<String>,
    integration_verify_effort: Option<String>,
    code_reviewer_model: Option<String>,
    code_reviewer_effort: Option<String>,
    contract_writer_model: Option<String>,
    contract_writer_effort: Option<String>,
}

impl ModelsSection {
    /// Parse every `[models]` key out of `doc`, validating each against its
    /// value set.
    pub(super) fn parse(doc: &DocumentMut) -> Result<Self> {
        let efforts = crate::models::stage::ALLOWED_REASONING_EFFORTS;
        let models = crate::claude::CLAUDE_MODELS;
        Ok(Self {
            standard_model: parse::get_enum(doc, "models", "standard_model", models)?,
            standard_effort: parse::get_enum(doc, "models", "standard_effort", efforts)?,
            knowledge_model: parse::get_enum(doc, "models", "knowledge_model", models)?,
            knowledge_effort: parse::get_enum(doc, "models", "knowledge_effort", efforts)?,
            knowledge_distill_model: parse::get_enum(
                doc,
                "models",
                "knowledge_distill_model",
                models,
            )?,
            knowledge_distill_effort: parse::get_enum(
                doc,
                "models",
                "knowledge_distill_effort",
                efforts,
            )?,
            integration_verify_model: parse::get_enum(
                doc,
                "models",
                "integration_verify_model",
                models,
            )?,
            integration_verify_effort: parse::get_enum(
                doc,
                "models",
                "integration_verify_effort",
                efforts,
            )?,
            code_reviewer_model: parse::get_enum(doc, "models", "code_reviewer_model", models)?,
            code_reviewer_effort: parse::get_enum(doc, "models", "code_reviewer_effort", efforts)?,
            contract_writer_model: parse::get_enum(doc, "models", "contract_writer_model", models)?,
            contract_writer_effort: parse::get_enum(
                doc,
                "models",
                "contract_writer_effort",
                efforts,
            )?,
        })
    }

    fn model_for(&self, stage_type: StageType) -> Option<&str> {
        match stage_type {
            StageType::Standard => self.standard_model.as_deref(),
            StageType::Knowledge => self.knowledge_model.as_deref(),
            StageType::KnowledgeDistill => self.knowledge_distill_model.as_deref(),
            StageType::IntegrationVerify => self.integration_verify_model.as_deref(),
        }
    }

    fn effort_for(&self, stage_type: StageType) -> Option<&str> {
        match stage_type {
            StageType::Standard => self.standard_effort.as_deref(),
            StageType::Knowledge => self.knowledge_effort.as_deref(),
            StageType::KnowledgeDistill => self.knowledge_distill_effort.as_deref(),
            StageType::IntegrationVerify => self.integration_verify_effort.as_deref(),
        }
    }
}

impl UserConfig {
    /// The model a `stage_type` main-agent session runs on absent a plan
    /// field and absent a project `[models]` section. Default: that stage
    /// type's [`StageType::default_model`].
    pub fn stage_model(&self, stage_type: StageType) -> &str {
        self.models
            .model_for(stage_type)
            .unwrap_or_else(|| stage_type.default_model())
    }

    /// The reasoning effort a `stage_type` main-agent session runs at absent
    /// a plan field and absent a project `[models]` section. Default: that
    /// stage type's [`StageType::default_reasoning_effort`].
    pub fn stage_reasoning_effort(&self, stage_type: StageType) -> &str {
        self.models
            .effort_for(stage_type)
            .unwrap_or_else(|| stage_type.default_reasoning_effort())
    }

    /// The model the `loom-code-reviewer` subagent runs on absent a project
    /// `[models]` override. Default: [`crate::claude::DEFAULT_CODE_REVIEWER_MODEL`].
    pub fn code_reviewer_model(&self) -> &str {
        self.models
            .code_reviewer_model
            .as_deref()
            .unwrap_or(crate::claude::DEFAULT_CODE_REVIEWER_MODEL)
    }

    /// The reasoning effort the `loom-code-reviewer` subagent runs at absent
    /// a project `[models]` override. Default:
    /// [`crate::claude::DEFAULT_CODE_REVIEWER_EFFORT`].
    pub fn code_reviewer_effort(&self) -> &str {
        self.models
            .code_reviewer_effort
            .as_deref()
            .unwrap_or(crate::claude::DEFAULT_CODE_REVIEWER_EFFORT)
    }

    /// The model the contract writer session runs on absent a project
    /// `[models]` override. Default: [`crate::claude::DEFAULT_CONTRACT_WRITER_MODEL`].
    pub fn contract_writer_model(&self) -> &str {
        self.models
            .contract_writer_model
            .as_deref()
            .unwrap_or(crate::claude::DEFAULT_CONTRACT_WRITER_MODEL)
    }

    /// The reasoning effort the contract writer session runs at absent a
    /// project `[models]` override. Default:
    /// [`crate::claude::DEFAULT_CONTRACT_WRITER_EFFORT`].
    pub fn contract_writer_effort(&self) -> &str {
        self.models
            .contract_writer_effort
            .as_deref()
            .unwrap_or(crate::claude::DEFAULT_CONTRACT_WRITER_EFFORT)
    }

    /// The typed value and origin for a `models.*` key, or `None` when
    /// `spec` is not one — the arm `UserConfig::value_of` delegates to.
    pub(super) fn models_value_of(&self, spec: &KeySpec) -> Option<(ConfigValue, Origin)> {
        let m = &self.models;
        let session_slot = match spec.name {
            "models.code_reviewer_model" => {
                Some((self.code_reviewer_model(), &m.code_reviewer_model))
            }
            "models.code_reviewer_effort" => {
                Some((self.code_reviewer_effort(), &m.code_reviewer_effort))
            }
            "models.contract_writer_model" => {
                Some((self.contract_writer_model(), &m.contract_writer_model))
            }
            "models.contract_writer_effort" => {
                Some((self.contract_writer_effort(), &m.contract_writer_effort))
            }
            _ => None,
        };
        if let Some((value, set)) = session_slot {
            return Some((
                ConfigValue::Text(value.to_string()),
                self.origin_of(set.as_ref()),
            ));
        }
        let (stage_type, is_effort) = match spec.name {
            "models.standard_model" => (StageType::Standard, false),
            "models.standard_effort" => (StageType::Standard, true),
            "models.knowledge_model" => (StageType::Knowledge, false),
            "models.knowledge_effort" => (StageType::Knowledge, true),
            "models.knowledge_distill_model" => (StageType::KnowledgeDistill, false),
            "models.knowledge_distill_effort" => (StageType::KnowledgeDistill, true),
            "models.integration_verify_model" => (StageType::IntegrationVerify, false),
            "models.integration_verify_effort" => (StageType::IntegrationVerify, true),
            _ => return None,
        };
        let (value, set) = if is_effort {
            (
                ConfigValue::Text(self.stage_reasoning_effort(stage_type).to_string()),
                self.models.effort_for(stage_type),
            )
        } else {
            (
                ConfigValue::Text(self.stage_model(stage_type).to_string()),
                self.models.model_for(stage_type),
            )
        };
        Some((value, self.origin_of(set)))
    }

    /// The `[models]` block of `loom config --print`, every key resolved.
    /// Ends with a single newline — [`UserConfig::to_toml_string`] supplies
    /// the blank line separating sections.
    pub(super) fn models_toml(&self) -> String {
        format!(
            "[models]\nstandard_model = {}\nstandard_effort = {}\nknowledge_model = {}\nknowledge_effort = {}\nknowledge_distill_model = {}\nknowledge_distill_effort = {}\nintegration_verify_model = {}\nintegration_verify_effort = {}\ncode_reviewer_model = {}\ncode_reviewer_effort = {}\ncontract_writer_model = {}\ncontract_writer_effort = {}\n",
            ConfigValue::Text(self.stage_model(StageType::Standard).to_string()).to_toml_literal(),
            ConfigValue::Text(self.stage_reasoning_effort(StageType::Standard).to_string())
                .to_toml_literal(),
            ConfigValue::Text(self.stage_model(StageType::Knowledge).to_string()).to_toml_literal(),
            ConfigValue::Text(self.stage_reasoning_effort(StageType::Knowledge).to_string())
                .to_toml_literal(),
            ConfigValue::Text(self.stage_model(StageType::KnowledgeDistill).to_string())
                .to_toml_literal(),
            ConfigValue::Text(
                self.stage_reasoning_effort(StageType::KnowledgeDistill)
                    .to_string()
            )
            .to_toml_literal(),
            ConfigValue::Text(self.stage_model(StageType::IntegrationVerify).to_string())
                .to_toml_literal(),
            ConfigValue::Text(
                self.stage_reasoning_effort(StageType::IntegrationVerify)
                    .to_string()
            )
            .to_toml_literal(),
            ConfigValue::Text(self.code_reviewer_model().to_string()).to_toml_literal(),
            ConfigValue::Text(self.code_reviewer_effort().to_string()).to_toml_literal(),
            ConfigValue::Text(self.contract_writer_model().to_string()).to_toml_literal(),
            ConfigValue::Text(self.contract_writer_effort().to_string()).to_toml_literal(),
        )
    }
}
