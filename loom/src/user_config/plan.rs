//! The `[plan]` section of the user configuration.

use anyhow::Result;
use toml_edit::DocumentMut;

use super::{parse, ConfigValue, KeySpec, Origin, UserConfig};

/// Default model for document summaries.
pub const DEFAULT_PLAN_DOCUMENT_SUMMARY_MODEL: &str = "haiku";
/// Default reasoning effort for document summaries.
pub const DEFAULT_PLAN_DOCUMENT_SUMMARY_EFFORT: &str = "medium";
/// Default model for overviews and summary sessions.
pub const DEFAULT_PLAN_OVERVIEW_MODEL: &str = "sonnet";
/// Default reasoning effort for overviews and summary sessions.
pub const DEFAULT_PLAN_OVERVIEW_EFFORT: &str = "high";

/// Values explicitly set in the user-tier `[plan]` section.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct PlanSection {
    document_summary_model: Option<String>,
    document_summary_effort: Option<String>,
    overview_model: Option<String>,
    overview_effort: Option<String>,
}

impl PlanSection {
    /// Parse and validate each `[plan]` value.
    pub(super) fn parse(doc: &DocumentMut) -> Result<Self> {
        let models = crate::claude::CLAUDE_MODELS;
        let efforts = crate::models::stage::ALLOWED_REASONING_EFFORTS;
        Ok(Self {
            document_summary_model: parse::get_enum(doc, "plan", "document_summary_model", models)?,
            document_summary_effort: parse::get_enum(
                doc,
                "plan",
                "document_summary_effort",
                efforts,
            )?,
            overview_model: parse::get_enum(doc, "plan", "overview_model", models)?,
            overview_effort: parse::get_enum(doc, "plan", "overview_effort", efforts)?,
        })
    }
}

impl UserConfig {
    /// Model used to build document summaries.
    pub fn plan_document_summary_model(&self) -> &str {
        self.plan
            .document_summary_model
            .as_deref()
            .unwrap_or(DEFAULT_PLAN_DOCUMENT_SUMMARY_MODEL)
    }

    /// Reasoning effort used to build document summaries.
    pub fn plan_document_summary_effort(&self) -> &str {
        self.plan
            .document_summary_effort
            .as_deref()
            .unwrap_or(DEFAULT_PLAN_DOCUMENT_SUMMARY_EFFORT)
    }

    /// Model used for overviews and summary sessions.
    pub fn plan_overview_model(&self) -> &str {
        self.plan
            .overview_model
            .as_deref()
            .unwrap_or(DEFAULT_PLAN_OVERVIEW_MODEL)
    }

    /// Reasoning effort used for overviews and summary sessions.
    pub fn plan_overview_effort(&self) -> &str {
        self.plan
            .overview_effort
            .as_deref()
            .unwrap_or(DEFAULT_PLAN_OVERVIEW_EFFORT)
    }

    /// Return the resolved value and origin for a `plan.*` key.
    pub(super) fn plan_value_of(&self, spec: &KeySpec) -> Option<(ConfigValue, Origin)> {
        Some(match spec.name {
            "plan.document_summary_model" => (
                ConfigValue::Text(self.plan_document_summary_model().to_string()),
                self.origin_of(self.plan.document_summary_model.as_ref()),
            ),
            "plan.document_summary_effort" => (
                ConfigValue::Text(self.plan_document_summary_effort().to_string()),
                self.origin_of(self.plan.document_summary_effort.as_ref()),
            ),
            "plan.overview_model" => (
                ConfigValue::Text(self.plan_overview_model().to_string()),
                self.origin_of(self.plan.overview_model.as_ref()),
            ),
            "plan.overview_effort" => (
                ConfigValue::Text(self.plan_overview_effort().to_string()),
                self.origin_of(self.plan.overview_effort.as_ref()),
            ),
            _ => return None,
        })
    }

    /// Render the resolved `[plan]` section with a final newline.
    pub(super) fn plan_toml(&self) -> String {
        format!(
            "[plan]\ndocument_summary_model = {}\ndocument_summary_effort = {}\noverview_model = {}\noverview_effort = {}\n",
            ConfigValue::Text(self.plan_document_summary_model().to_string()).to_toml_literal(),
            ConfigValue::Text(self.plan_document_summary_effort().to_string()).to_toml_literal(),
            ConfigValue::Text(self.plan_overview_model().to_string()).to_toml_literal(),
            ConfigValue::Text(self.plan_overview_effort().to_string()).to_toml_literal(),
        )
    }
}

#[cfg(test)]
mod tests;
