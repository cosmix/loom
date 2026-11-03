use super::*;

#[test]
fn defaults_apply_when_no_tier_sets_values() {
    let models = SummaryModels::resolve(&PlanConfig::default(), &UserConfig::default());

    assert_eq!(
        models,
        SummaryModels {
            document_model: "haiku".into(),
            document_effort: "high".into(),
            overview_model: "sonnet".into(),
            overview_effort: "high".into(),
            backend: "claude".into(),
            codex_model: "gpt-6.1-terra".into(),
            codex_effort: "high".into(),
        }
    );
}

#[test]
fn user_values_override_all_defaults() {
    let user = crate::user_config::parse_document(
        "[plan]\ndocument_summary_model = \"opus\"\ndocument_summary_effort = \"low\"\n\
         overview_model = \"haiku\"\noverview_effort = \"medium\"\n\
         summary_backend = \"codex\"\nsummary_codex_model = \"gpt-6.1-sol\"\n\
         summary_codex_effort = \"medium\"\n",
    )
    .unwrap();

    let models = SummaryModels::resolve(&PlanConfig::default(), &user);

    assert_eq!(
        models,
        SummaryModels {
            document_model: "opus".into(),
            document_effort: "low".into(),
            overview_model: "haiku".into(),
            overview_effort: "medium".into(),
            backend: "codex".into(),
            codex_model: "gpt-6.1-sol".into(),
            codex_effort: "medium".into(),
        }
    );
}

#[test]
fn project_values_override_user_values_for_every_slot() {
    let user = crate::user_config::parse_document(
        "[plan]\ndocument_summary_model = \"opus\"\ndocument_summary_effort = \"low\"\n\
         overview_model = \"haiku\"\noverview_effort = \"medium\"\n\
         summary_backend = \"codex\"\nsummary_codex_model = \"gpt-6.1-sol\"\n\
         summary_codex_effort = \"medium\"\n",
    )
    .unwrap();
    let project = PlanConfig::from_section_toml(
        "document_summary_model = \"sonnet\"\ndocument_summary_effort = \"high\"\n\
         overview_model = \"opus\"\noverview_effort = \"low\"\n\
         summary_backend = \"claude\"\nsummary_codex_model = \"gpt-6.1-terra\"\n\
         summary_codex_effort = \"xhigh\"\n",
    )
    .unwrap();

    let models = SummaryModels::resolve(&project, &user);

    assert_eq!(
        models,
        SummaryModels {
            document_model: "sonnet".into(),
            document_effort: "high".into(),
            overview_model: "opus".into(),
            overview_effort: "low".into(),
            backend: "claude".into(),
            codex_model: "gpt-6.1-terra".into(),
            codex_effort: "xhigh".into(),
        }
    );
}

#[test]
fn missing_and_invalid_project_values_fall_back_independently() {
    let user = crate::user_config::parse_document(
        "[plan]\ndocument_summary_effort = \"low\"\noverview_model = \"opus\"\n\
         summary_backend = \"codex\"\nsummary_codex_model = \"gpt-6.1-sol\"\n",
    )
    .unwrap();
    let project = PlanConfig::from_section_toml(
        "document_summary_model = \"sonnet\"\noverview_model = \"invalid\"\noverview_effort = \"low\"\n\
         summary_backend = \"invalid\"\nsummary_codex_effort = \"invalid\"\n",
    ).unwrap();

    let models = SummaryModels::resolve(&project, &user);

    assert_eq!(
        models,
        SummaryModels {
            document_model: "sonnet".into(),
            document_effort: "low".into(),
            overview_model: "opus".into(),
            overview_effort: "low".into(),
            backend: "codex".into(),
            codex_model: "gpt-6.1-sol".into(),
            codex_effort: "high".into(),
        }
    );
}
