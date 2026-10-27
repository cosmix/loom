//! Project-tier summary model settings from the `[planning]` table.

use std::path::Path;

use crate::models::stage::ALLOWED_REASONING_EFFORTS;

use super::allowed::allowed_value;

const PLANNING_SECTION: &str = "planning";

/// Project-level overrides for summary models and reasoning efforts.
#[derive(Debug, Clone, Default, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PlanConfig {
    document_summary_model: Option<String>,
    document_summary_effort: Option<String>,
    overview_model: Option<String>,
    overview_effort: Option<String>,
}

impl PlanConfig {
    /// Parse a `[planning]` table body without accessing the filesystem.
    pub fn from_section_toml(body: &str) -> anyhow::Result<Self> {
        toml::from_str(body)
            .map_err(|e| anyhow::anyhow!("failed to parse [planning] section body: {e}"))
    }

    /// Return the document summary model if it is allowed.
    pub fn document_summary_model(&self) -> Option<&str> {
        allowed_value(
            self.document_summary_model.as_ref(),
            crate::claude::CLAUDE_MODELS,
            "plan.document_summary_model",
        )
    }

    /// Return the document summary reasoning effort if it is allowed.
    pub fn document_summary_effort(&self) -> Option<&str> {
        allowed_value(
            self.document_summary_effort.as_ref(),
            ALLOWED_REASONING_EFFORTS,
            "plan.document_summary_effort",
        )
    }

    /// Return the overview model if it is allowed.
    pub fn overview_model(&self) -> Option<&str> {
        allowed_value(
            self.overview_model.as_ref(),
            crate::claude::CLAUDE_MODELS,
            "plan.overview_model",
        )
    }

    /// Return the overview reasoning effort if it is allowed.
    pub fn overview_effort(&self) -> Option<&str> {
        allowed_value(
            self.overview_effort.as_ref(),
            ALLOWED_REASONING_EFFORTS,
            "plan.overview_effort",
        )
    }
}

/// Read `[planning]`, falling back to an empty config if it cannot be read.
pub fn read_plan_config(work_dir: &Path) -> PlanConfig {
    super::read_section::<PlanConfig>(work_dir, PLANNING_SECTION)
        .ok()
        .flatten()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
