//! Native JSON value shapes and invalid project-value recovery.

use crate::fs::work_dir::{read_config, resolve_stage_model_effort};
use crate::models::stage::StageType;
use crate::user_config::keys::{spec, ValueKind};
use crate::user_config::ConfigValue;

use super::super::update;
use super::super::wire::{ConfigKind, ConfigUpdated, Source};
use super::{entry, parse, scratch, Scratch};

fn post_ok(scratch: &Scratch, body: &str) -> ConfigUpdated {
    let (status, response) = update(&scratch.base, body.as_bytes());
    assert_eq!(status, 200, "{response}");
    serde_json::from_str(&response).expect("response is a ConfigUpdated")
}

fn repair_invalid_project_value(scratch: &Scratch) {
    crate::user_config::set(
        spec("models.standard_model").unwrap(),
        ConfigValue::Text("haiku".to_owned()),
    )
    .expect("set the user standard model");

    let written = post_ok(
        scratch,
        r#"{"scope":"project","name":"models.standard_model","value":"opus"}"#,
    );
    assert_eq!(written.old, ConfigValue::Text("haiku".to_owned()));
    assert_eq!(written.new, ConfigValue::Text("opus".to_owned()));

    let cleared = post_ok(
        scratch,
        r#"{"scope":"project","name":"models.standard_model","value":null}"#,
    );
    assert_eq!(cleared.old, ConfigValue::Text("opus".to_owned()));
    assert_eq!(cleared.new, ConfigValue::Text("haiku".to_owned()));
    let text = read_config(&scratch.work())
        .expect("read the workspace config")
        .to_string();
    assert!(!text.contains("standard_model"), "{text}");
}

#[test]
fn a_bool_key_ships_a_json_boolean() {
    let scratch = scratch();
    let payload = serde_json::to_value(parse(&scratch.base)).expect("serialize the payload");
    let entry = payload["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .find(|entry| entry["name"] == "update.check")
        .expect("the update check entry");

    assert!(entry["user"]["value"].is_boolean());
    assert!(entry["effective"]["value"].is_boolean());
}

#[test]
fn a_number_key_ships_a_json_number() {
    let scratch = scratch();
    let payload = serde_json::to_value(parse(&scratch.base)).expect("serialize the payload");
    let entries = payload["entries"].as_array().expect("entries");
    let numbers: Vec<&serde_json::Value> = entries
        .iter()
        .filter(|entry| entry["kind"]["type"] == "number")
        .collect();

    assert_eq!(numbers.len(), 2);
    for entry in numbers {
        assert!(entry["user"]["value"].is_number(), "{entry}");
        assert!(entry["effective"]["value"].is_number(), "{entry}");
    }
}

#[test]
fn a_string_for_a_bool_key_is_rejected_by_checked() {
    let scratch = scratch();
    let (status, _) = update(
        &scratch.base,
        br#"{"scope":"user","name":"update.check","value":"false"}"#,
    );

    assert_eq!(status, 400);
}

#[test]
fn a_number_for_an_enum_key_reports_the_registry_message() {
    let scratch = scratch();
    let (status, body) = update(
        &scratch.base,
        br#"{"scope":"user","name":"terminal.backend","value":42}"#,
    );

    assert_eq!(status, 400);
    assert!(
        body.contains("is not one of the expected values: native, tmux"),
        "{body}"
    );
}

#[test]
fn a_negative_number_is_rejected_before_checked() {
    let scratch = scratch();
    let (status, _) = update(
        &scratch.base,
        br#"{"scope":"project","name":"context.ceiling_tokens","value":-1}"#,
    );

    assert_eq!(status, 400);
}

#[test]
fn the_string_kind_serializes_as_type_string() {
    let kind = serde_json::to_value(ConfigKind::from(&ValueKind::String))
        .expect("serialize the string kind");

    assert_eq!(kind, serde_json::json!({ "type": "string" }));
}

#[test]
fn an_off_list_project_value_falls_through_like_the_daemon() {
    let scratch = scratch();
    crate::user_config::set(
        spec("models.standard_model").unwrap(),
        ConfigValue::Text("haiku".to_owned()),
    )
    .expect("set the user standard model");
    scratch.write_project(
        "models",
        "standard_model",
        toml_edit::Value::from("claude-opus-5"),
    );

    let payload = parse(&scratch.base);
    let model = entry(&payload, "models.standard_model");
    assert!(model.project.as_ref().expect("project scope").set);
    assert_eq!(
        model.project.as_ref().unwrap().value,
        ConfigValue::Text("haiku".to_owned())
    );
    assert_ne!(model.effective.source, Source::Project);
    let expected = resolve_stage_model_effort(&scratch.work(), StageType::Standard, None, None).0;
    assert_eq!(model.effective.value, ConfigValue::Text(expected));
}

#[test]
fn a_non_string_project_value_drops_its_section_like_the_daemon() {
    let scratch = scratch();
    scratch.write_project("models", "standard_model", toml_edit::Value::from(42_i64));
    scratch.write_project("models", "standard_effort", toml_edit::Value::from("low"));

    let payload = parse(&scratch.base);
    let model = entry(&payload, "models.standard_model");
    let effort = entry(&payload, "models.standard_effort");
    assert_ne!(model.effective.source, Source::Project);
    assert_ne!(effort.effective.source, Source::Project);
    let expected = resolve_stage_model_effort(&scratch.work(), StageType::Standard, None, None).1;
    assert_eq!(effort.effective.value, ConfigValue::Text(expected));
}

#[test]
fn an_invalid_project_value_can_be_replaced_and_cleared() {
    {
        let scratch = scratch();
        scratch.write_project(
            "models",
            "standard_model",
            toml_edit::Value::from("claude-opus-5"),
        );
        repair_invalid_project_value(&scratch);
    }
    {
        let scratch = scratch();
        scratch.write_project("models", "standard_model", toml_edit::Value::from(42_i64));
        scratch.write_project("models", "standard_effort", toml_edit::Value::from("low"));
        repair_invalid_project_value(&scratch);
    }
}
