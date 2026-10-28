//! Resolved model and effort settings for plan authoring sessions.

use super::UserConfig;

impl UserConfig {
    /// Model used for spec authoring steps.
    pub fn plan_spec_authoring_model(&self) -> &str {
        self.plan.spec_authoring_model.as_deref().unwrap_or("opus")
    }

    /// Reasoning effort used for spec authoring steps.
    pub fn plan_spec_authoring_effort(&self) -> &str {
        self.plan
            .spec_authoring_effort
            .as_deref()
            .unwrap_or("medium")
    }

    /// Model used for plan authoring steps.
    pub fn plan_plan_authoring_model(&self) -> &str {
        self.plan.plan_authoring_model.as_deref().unwrap_or("opus")
    }

    /// Reasoning effort used for plan authoring steps.
    pub fn plan_plan_authoring_effort(&self) -> &str {
        self.plan.plan_authoring_effort.as_deref().unwrap_or("high")
    }
}

#[cfg(test)]
mod tests {
    use super::UserConfig;
    use crate::user_config::{keys::spec, parse_document, Origin};

    #[test]
    fn authoring_defaults() {
        let config = UserConfig::default();
        assert_eq!(config.plan_spec_authoring_model(), "opus");
        assert_eq!(config.plan_spec_authoring_effort(), "medium");
        assert_eq!(config.plan_plan_authoring_model(), "opus");
        assert_eq!(config.plan_plan_authoring_effort(), "high");
    }

    #[test]
    fn configured_authoring_values_and_origin() {
        let config = parse_document(
            "[plan]\nspec_authoring_model = \"sonnet\"\nplan_authoring_effort = \"low\"\n",
        )
        .unwrap();
        assert_eq!(config.plan_spec_authoring_model(), "sonnet");
        assert_eq!(config.plan_plan_authoring_effort(), "low");
        let (value, origin) = config.value_of(spec("plan.spec_authoring_model").unwrap());
        assert_eq!(value.to_string(), "sonnet");
        assert_eq!(origin, Origin::Set);
    }

    #[test]
    fn rejects_unknown_authoring_model() {
        assert!(parse_document("[plan]\nplan_authoring_model = \"gpt-5\"\n").is_err());
    }

    #[test]
    fn authoring_keys_accept_known_values_and_reject_unknown_variants() {
        for (name, valid) in [
            ("plan.spec_authoring_model", "opus"),
            ("plan.spec_authoring_effort", "high"),
            ("plan.plan_authoring_model", "opus"),
            ("plan.plan_authoring_effort", "high"),
        ] {
            let key = spec(name).unwrap();
            assert!(key.parse(valid).is_ok(), "{name} rejected {valid}");
            assert!(key.parse("bogus").is_err(), "{name} accepted bogus");
        }
    }
}
