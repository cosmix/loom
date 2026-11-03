use super::*;
use crate::user_config::parse_document;

#[test]
fn defaults_are_used_for_unset_plan_values() {
    let config = UserConfig::default();
    assert_eq!(config.plan_document_summary_model(), "haiku");
    assert_eq!(config.plan_document_summary_effort(), "high");
    assert_eq!(config.plan_overview_model(), "sonnet");
    assert_eq!(config.plan_overview_effort(), "high");
    assert_eq!(config.plan_summary_backend(), "claude");
    assert_eq!(config.plan_summary_codex_model(), "gpt-6.1-terra");
    assert_eq!(config.plan_summary_codex_effort(), "high");
}

#[test]
fn set_plan_values_are_returned() {
    let config = parse_document(
        "[plan]\ndocument_summary_model = \"opus\"\ndocument_summary_effort = \"low\"\n\
         overview_model = \"haiku\"\noverview_effort = \"medium\"\n",
    )
    .unwrap();
    assert_eq!(config.plan_document_summary_model(), "opus");
    assert_eq!(config.plan_document_summary_effort(), "low");
    assert_eq!(config.plan_overview_model(), "haiku");
    assert_eq!(config.plan_overview_effort(), "medium");
}

#[test]
fn unknown_plan_variants_name_the_key() {
    for key in [
        "document_summary_model",
        "document_summary_effort",
        "overview_model",
        "overview_effort",
    ] {
        let text = format!("[plan]\n{key} = \"invalid-variant\"\n");
        let error = parse_document(&text).unwrap_err().to_string();
        assert!(error.contains(&format!("plan.{key}")), "{error}");
        assert!(error.contains("invalid-variant"), "{error}");
    }
}

#[test]
fn plan_toml_lists_all_resolved_keys() {
    let rendered = UserConfig::default().plan_toml();
    assert_eq!(
        rendered,
        "[plan]\ndocument_summary_model = \"haiku\"\ndocument_summary_effort = \"high\"\noverview_model = \"sonnet\"\noverview_effort = \"high\"\nsummary_backend = \"claude\"\nsummary_codex_model = \"gpt-6.1-terra\"\nsummary_codex_effort = \"high\"\nspec_authoring_model = \"opus\"\nspec_authoring_effort = \"medium\"\nplan_authoring_model = \"opus\"\nplan_authoring_effort = \"high\"\n"
    );
}
