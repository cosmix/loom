use super::*;

fn pair(model: &str, effort: &str) -> ModelEffort {
    ModelEffort {
        model: model.into(),
        effort: effort.into(),
    }
}

#[test]
fn defaults_apply_when_no_tier_sets_values() {
    let models = GuidanceModels::resolve(&PlanConfig::default(), &UserConfig::default());

    assert_eq!(
        models,
        GuidanceModels {
            spec: pair("opus", "medium"),
            plan: pair("opus", "high"),
            summary_session: pair("sonnet", "high"),
            document_agent: pair("haiku", "high"),
            overview_agent: pair("sonnet", "high"),
        }
    );
}

#[test]
fn user_values_override_all_defaults() {
    let user = crate::user_config::parse_document(
        r#"[plan]
spec_authoring_model = "sonnet"
spec_authoring_effort = "high"
plan_authoring_model = "haiku"
plan_authoring_effort = "medium"
document_summary_model = "opus"
document_summary_effort = "low"
overview_model = "haiku"
overview_effort = "medium"
"#,
    )
    .unwrap();

    let models = GuidanceModels::resolve(&PlanConfig::default(), &user);

    assert_eq!(
        models,
        GuidanceModels {
            spec: pair("sonnet", "high"),
            plan: pair("haiku", "medium"),
            summary_session: pair("haiku", "medium"),
            document_agent: pair("opus", "low"),
            overview_agent: pair("haiku", "medium"),
        }
    );
}

#[test]
fn project_values_override_user_values_for_every_slot() {
    let user = crate::user_config::parse_document(
        r#"[plan]
spec_authoring_model = "sonnet"
spec_authoring_effort = "high"
plan_authoring_model = "haiku"
plan_authoring_effort = "medium"
document_summary_model = "opus"
document_summary_effort = "low"
overview_model = "haiku"
overview_effort = "medium"
"#,
    )
    .unwrap();
    let project = PlanConfig::from_section_toml(
        r#"spec_authoring_model = "haiku"
spec_authoring_effort = "low"
plan_authoring_model = "sonnet"
plan_authoring_effort = "high"
document_summary_model = "sonnet"
document_summary_effort = "high"
overview_model = "opus"
overview_effort = "low"
"#,
    )
    .unwrap();

    let models = GuidanceModels::resolve(&project, &user);

    assert_eq!(
        models,
        GuidanceModels {
            spec: pair("haiku", "low"),
            plan: pair("sonnet", "high"),
            summary_session: pair("opus", "low"),
            document_agent: pair("sonnet", "high"),
            overview_agent: pair("opus", "low"),
        }
    );
}

#[test]
fn missing_and_invalid_project_values_fall_back_independently() {
    let user = crate::user_config::parse_document(
        r#"[plan]
spec_authoring_model = "sonnet"
spec_authoring_effort = "low"
plan_authoring_model = "haiku"
plan_authoring_effort = "medium"
"#,
    )
    .unwrap();
    let project = PlanConfig::from_section_toml(
        r#"spec_authoring_model = "opus"
spec_authoring_effort = "invalid"
plan_authoring_model = "invalid"
plan_authoring_effort = "high"
"#,
    )
    .unwrap();

    let models = GuidanceModels::resolve(&project, &user);

    assert_eq!(
        models,
        GuidanceModels {
            spec: pair("opus", "low"),
            plan: pair("haiku", "high"),
            summary_session: pair("sonnet", "high"),
            document_agent: pair("haiku", "high"),
            overview_agent: pair("sonnet", "high"),
        }
    );
}
