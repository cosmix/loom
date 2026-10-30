//! Project-tier model settings from the `[planning]` table.

use std::path::Path;

use crate::models::stage::ALLOWED_REASONING_EFFORTS;

use super::allowed::allowed_value;

const PLANNING_SECTION: &str = "planning";

/// Project-level overrides for models and reasoning efforts.
#[derive(Debug, Clone, Default, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PlanConfig {
    document_summary_model: Option<String>,
    document_summary_effort: Option<String>,
    overview_model: Option<String>,
    overview_effort: Option<String>,
    summary_backend: Option<String>,
    summary_codex_model: Option<String>,
    summary_codex_effort: Option<String>,
    spec_authoring_model: Option<String>,
    spec_authoring_effort: Option<String>,
    plan_authoring_model: Option<String>,
    plan_authoring_effort: Option<String>,
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

    /// Return the summary backend if it is allowed.
    pub fn summary_backend(&self) -> Option<&str> {
        allowed_value(
            self.summary_backend.as_ref(),
            crate::user_config::SUMMARY_BACKENDS,
            "plan.summary_backend",
        )
    }

    /// Return the summary Codex model if it is allowed.
    pub fn summary_codex_model(&self) -> Option<&str> {
        allowed_value(
            self.summary_codex_model.as_ref(),
            crate::codex::CODEX_MODELS,
            "plan.summary_codex_model",
        )
    }

    /// Return the summary Codex reasoning effort if it is allowed.
    pub fn summary_codex_effort(&self) -> Option<&str> {
        allowed_value(
            self.summary_codex_effort.as_ref(),
            crate::codex::CODEX_EFFORTS,
            "plan.summary_codex_effort",
        )
    }

    /// Return the spec authoring model if it is allowed.
    pub fn spec_authoring_model(&self) -> Option<&str> {
        allowed_value(
            self.spec_authoring_model.as_ref(),
            crate::claude::CLAUDE_MODELS,
            "plan.spec_authoring_model",
        )
    }

    /// Return the spec authoring reasoning effort if it is allowed.
    pub fn spec_authoring_effort(&self) -> Option<&str> {
        allowed_value(
            self.spec_authoring_effort.as_ref(),
            ALLOWED_REASONING_EFFORTS,
            "plan.spec_authoring_effort",
        )
    }

    /// Return the plan authoring model if it is allowed.
    pub fn plan_authoring_model(&self) -> Option<&str> {
        allowed_value(
            self.plan_authoring_model.as_ref(),
            crate::claude::CLAUDE_MODELS,
            "plan.plan_authoring_model",
        )
    }

    /// Return the plan authoring reasoning effort if it is allowed.
    pub fn plan_authoring_effort(&self) -> Option<&str> {
        allowed_value(
            self.plan_authoring_effort.as_ref(),
            ALLOWED_REASONING_EFFORTS,
            "plan.plan_authoring_effort",
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

#[cfg(test)]
mod authoring_tests {
    use super::*;

    #[test]
    fn summary_accessors_return_valid_values() {
        let config = PlanConfig::from_section_toml(
            "summary_backend = \"codex\"\n\
             summary_codex_model = \"gpt-6.1-sol\"\n\
             summary_codex_effort = \"xhigh\"\n",
        )
        .unwrap();
        assert_eq!(config.summary_backend(), Some("codex"));
        assert_eq!(config.summary_codex_model(), Some("gpt-6.1-sol"));
        assert_eq!(config.summary_codex_effort(), Some("xhigh"));

        let config = PlanConfig::from_section_toml("summary_backend = \"claude\"").unwrap();
        assert_eq!(config.summary_backend(), Some("claude"));
    }

    #[test]
    fn summary_accessors_drop_invalid_values() {
        let config = PlanConfig::from_section_toml(
            "summary_backend = \"sonnet\"\n\
             summary_codex_model = \"sonnet\"\n\
             summary_codex_effort = \"max\"\n",
        )
        .unwrap();
        assert_eq!(config.summary_backend(), None);
        assert_eq!(config.summary_codex_model(), None);
        assert_eq!(config.summary_codex_effort(), None);
    }

    #[test]
    fn summary_accessors_return_none_when_omitted() {
        let config = PlanConfig::from_section_toml("").unwrap();
        assert_eq!(config.summary_backend(), None);
        assert_eq!(config.summary_codex_model(), None);
        assert_eq!(config.summary_codex_effort(), None);
    }

    #[test]
    fn authoring_accessors_return_valid_values() {
        let config = PlanConfig::from_section_toml(
            "spec_authoring_model = \"sonnet\"\n\
             spec_authoring_effort = \"medium\"\n\
             plan_authoring_model = \"opus\"\n\
             plan_authoring_effort = \"high\"\n",
        )
        .unwrap();
        assert_eq!(config.spec_authoring_model(), Some("sonnet"));
        assert_eq!(config.spec_authoring_effort(), Some("medium"));
        assert_eq!(config.plan_authoring_model(), Some("opus"));
        assert_eq!(config.plan_authoring_effort(), Some("high"));
    }

    #[test]
    fn authoring_accessors_drop_invalid_values() {
        let config = PlanConfig::from_section_toml(
            "spec_authoring_model = \"bogus\"\n\
             spec_authoring_effort = \"bogus\"\n\
             plan_authoring_model = \"bogus\"\n\
             plan_authoring_effort = \"bogus\"\n",
        )
        .unwrap();
        assert_eq!(config.spec_authoring_model(), None);
        assert_eq!(config.spec_authoring_effort(), None);
        assert_eq!(config.plan_authoring_model(), None);
        assert_eq!(config.plan_authoring_effort(), None);
    }

    #[test]
    fn authoring_model_is_read_alongside_run_plan() {
        let temp = tempfile::TempDir::new().unwrap();
        std::fs::write(
            temp.path().join("config.toml"),
            "[plan]\nsource_path = \"doc/plan.md\"\nplan_id = \"run\"\n\
             plan_name = \"Run\"\nbase_branch = \"main\"\n\
             [planning]\nplan_authoring_model = \"sonnet\"\n",
        )
        .unwrap();
        assert_eq!(
            read_plan_config(temp.path()).plan_authoring_model(),
            Some("sonnet")
        );
    }
}
