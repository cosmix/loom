use super::*;

#[test]
fn falls_back_to_the_stage_type_built_in_when_nothing_is_set() {
    let project = ModelsConfig::default();
    let user = UserConfig::default();
    let cases = [
        (StageType::Standard, "sonnet", "high"),
        (StageType::Knowledge, "opus", "medium"),
        (StageType::KnowledgeDistill, "sonnet", "high"),
        (StageType::IntegrationVerify, "opus", "xhigh"),
    ];
    for (stage_type, model, effort) in cases {
        assert_eq!(
            resolve_from(&project, &user, stage_type, None, None),
            (model.to_string(), effort.to_string())
        );
    }
}

#[test]
fn a_project_key_wins_over_the_built_in() {
    let project =
        ModelsConfig::from_section_toml("standard_model = \"sonnet\"\nstandard_effort = \"low\"\n")
            .unwrap();
    let user = UserConfig::default();
    assert_eq!(
        resolve_from(&project, &user, StageType::Standard, None, None),
        ("sonnet".to_string(), "low".to_string())
    );
}

#[test]
fn a_plan_field_wins_over_a_project_key() {
    let project =
        ModelsConfig::from_section_toml("standard_model = \"sonnet\"\nstandard_effort = \"low\"\n")
            .unwrap();
    let user = UserConfig::default();
    assert_eq!(
        resolve_from(
            &project,
            &user,
            StageType::Standard,
            Some("haiku"),
            Some("xhigh")
        ),
        ("haiku".to_string(), "xhigh".to_string())
    );
}

#[test]
fn a_project_key_absent_from_a_present_section_falls_through() {
    // Only standard_model is set; standard_effort must still resolve to
    // the built-in rather than being shadowed by the present section.
    let project = ModelsConfig::from_section_toml("standard_model = \"sonnet\"\n").unwrap();
    let user = UserConfig::default();
    assert_eq!(
        resolve_from(&project, &user, StageType::Standard, None, None),
        ("sonnet".to_string(), "high".to_string())
    );
}

#[test]
fn an_out_of_set_project_value_is_ignored() {
    let project = ModelsConfig::from_section_toml("standard_model = \"nonexistent\"\n").unwrap();
    let user = UserConfig::default();
    assert_eq!(
        resolve_from(&project, &user, StageType::Standard, None, None),
        ("sonnet".to_string(), "high".to_string())
    );
}

#[test]
fn code_reviewer_defaults_to_opus_high() {
    let (model, effort) =
        resolve_code_reviewer_from(&ModelsConfig::default(), &UserConfig::default());
    assert_eq!((model.as_str(), effort.as_str()), ("opus", "high"));
}

#[test]
fn code_reviewer_project_beats_user_beats_default_per_key() {
    let user = crate::user_config::parse_document(
        "[models]\ncode_reviewer_model = \"haiku\"\ncode_reviewer_effort = \"low\"\n",
    )
    .unwrap();
    let project = ModelsConfig::from_section_toml("code_reviewer_model = \"sonnet\"\n").unwrap();
    // Model from the project, effort falls through to the user tier.
    assert_eq!(
        resolve_code_reviewer_from(&project, &user),
        ("sonnet".to_string(), "low".to_string())
    );
    // No project keys: the user tier wins over the built-in.
    assert_eq!(
        resolve_code_reviewer_from(&ModelsConfig::default(), &user),
        ("haiku".to_string(), "low".to_string())
    );
}

#[test]
fn contract_writer_defaults_to_opus_high() {
    let (model, effort) =
        resolve_contract_writer_from(&ModelsConfig::default(), &UserConfig::default());
    assert_eq!((model.as_str(), effort.as_str()), ("opus", "high"));
}

#[test]
fn contract_writer_project_beats_user_beats_default_per_key() {
    let user = crate::user_config::parse_document(
        "[models]\ncontract_writer_model = \"haiku\"\ncontract_writer_effort = \"low\"\n",
    )
    .unwrap();
    let project = ModelsConfig::from_section_toml("contract_writer_model = \"sonnet\"\n").unwrap();
    assert_eq!(
        resolve_contract_writer_from(&project, &user),
        ("sonnet".to_string(), "low".to_string())
    );
    assert_eq!(
        resolve_contract_writer_from(&ModelsConfig::default(), &user),
        ("haiku".to_string(), "low".to_string())
    );
}

#[test]
fn project_tier_wins_over_user_tier_and_falls_through_key_by_key() {
    let temp = tempfile::TempDir::new().unwrap();
    let work_dir = temp.path().join("work");
    std::fs::create_dir_all(&work_dir).unwrap();
    std::fs::write(
        work_dir.join("config.toml"),
        "[models]\nstandard_model = \"sonnet\"\n",
    )
    .unwrap();

    let user_config_path = temp.path().join("user-config.toml");
    std::fs::write(
        &user_config_path,
        "[models]\nstandard_model = \"haiku\"\nstandard_effort = \"low\"\n",
    )
    .unwrap();
    let _redirect = crate::user_config::redirect_user_config(user_config_path);

    // The project's standard_model wins over the user config's.
    // standard_effort, which the project section omits, falls through to
    // the user config rather than the built-in.
    assert_eq!(
        resolve_stage_model_effort(&work_dir, StageType::Standard, None, None),
        ("sonnet".to_string(), "low".to_string())
    );
}

#[test]
fn strict_code_reviewer_resolution_reads_both_tiers_and_errors_on_a_bad_file() {
    let temp = tempfile::TempDir::new().unwrap();
    let work_dir = temp.path().join("work");
    std::fs::create_dir_all(&work_dir).unwrap();
    let user_config_path = temp.path().join("user-config.toml");
    std::fs::write(
        &user_config_path,
        "[models]\ncode_reviewer_effort = \"low\"\n",
    )
    .unwrap();
    let _redirect = crate::user_config::redirect_user_config(user_config_path);

    assert_eq!(
        resolve_code_reviewer_model_effort(None).unwrap(),
        ("opus".to_string(), "low".to_string())
    );
    std::fs::write(
        work_dir.join("config.toml"),
        "[models]\ncode_reviewer_model = \"sonnet\"\n",
    )
    .unwrap();
    assert_eq!(
        resolve_code_reviewer_model_effort(Some(&work_dir)).unwrap(),
        ("sonnet".to_string(), "low".to_string())
    );
    std::fs::write(work_dir.join("config.toml"), "[models\n").unwrap();
    assert!(resolve_code_reviewer_model_effort(Some(&work_dir)).is_err());
}
