use super::*;

#[test]
fn planning_values_are_read_alongside_the_run_plan() {
    let temp = tempfile::TempDir::new().unwrap();
    std::fs::write(
        temp.path().join("config.toml"),
        "[plan]\nsource_path = \"doc/plan.md\"\nplan_id = \"run\"\n\
         [planning]\ndocument_summary_model = \"haiku\"\n\
         document_summary_effort = \"medium\"\noverview_model = \"sonnet\"\n\
         overview_effort = \"high\"\n",
    )
    .unwrap();

    let config = read_plan_config(temp.path());
    assert_eq!(config.document_summary_model(), Some("haiku"));
    assert_eq!(config.document_summary_effort(), Some("medium"));
    assert_eq!(config.overview_model(), Some("sonnet"));
    assert_eq!(config.overview_effort(), Some("high"));
}

#[test]
fn invalid_values_fall_through_individually() {
    let config = PlanConfig::from_section_toml(
        "document_summary_model = \"invalid\"\ndocument_summary_effort = \"invalid\"\n\
         overview_model = \"invalid\"\noverview_effort = \"invalid\"\n",
    )
    .unwrap();
    assert_eq!(config.document_summary_model(), None);
    assert_eq!(config.document_summary_effort(), None);
    assert_eq!(config.overview_model(), None);
    assert_eq!(config.overview_effort(), None);
}

#[test]
fn unknown_key_rejects_the_section() {
    let body = "overview_model = \"sonnet\"\nbogus_key = \"nope\"\n";
    assert!(PlanConfig::from_section_toml(body).is_err());
    let temp = tempfile::TempDir::new().unwrap();
    std::fs::write(
        temp.path().join("config.toml"),
        format!("[planning]\n{body}"),
    )
    .unwrap();
    assert_eq!(read_plan_config(temp.path()), PlanConfig::default());
}

#[test]
fn missing_config_yields_defaults() {
    let temp = tempfile::TempDir::new().unwrap();
    assert_eq!(read_plan_config(temp.path()), PlanConfig::default());
}
