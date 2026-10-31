use super::*;
use crate::fs::work_dir::{read_pressure_config, PressureConfig};

fn pressure_values(config: &PressureConfig) -> [(&'static str, Option<ConfigValue>); 7] {
    let text = |value: Option<&str>| value.map(|value| ConfigValue::Text(value.to_owned()));
    [
        ("pressure.claude_model", text(config.claude_model())),
        ("pressure.claude_effort", text(config.claude_effort())),
        ("pressure.codex_model", text(config.codex_model())),
        ("pressure.codex_effort", text(config.codex_effort())),
        ("pressure.address_model", text(config.address_model())),
        ("pressure.address_effort", text(config.address_effort())),
        ("pressure.rounds", config.rounds().map(ConfigValue::Number)),
    ]
}

#[test]
fn pressure_projection_matches_runtime_for_invalid_and_valid_sections() {
    for (model, rounds, unknown) in [
        ("sonnet", 3_i64, false),
        ("not-a-model", 3, false),
        ("sonnet", 3, true),
        ("sonnet", -1, false),
        ("sonnet", i64::from(u32::MAX) + 1, false),
        ("sonnet", 0, false),
    ] {
        let scratch = scratch();
        crate::user_config::set(
            spec("pressure.claude_model").unwrap(),
            ConfigValue::Text("haiku".to_owned()),
        )
        .unwrap();
        crate::user_config::set(spec("pressure.rounds").unwrap(), ConfigValue::Number(2)).unwrap();
        scratch.write_project("pressure", "claude_model", toml_edit::Value::from(model));
        scratch.write_project("pressure", "rounds", toml_edit::Value::from(rounds));
        if unknown {
            scratch.write_project("pressure", "unexpected", toml_edit::Value::from("value"));
        }
        let payload = parse(&scratch.base);
        let config = read_pressure_config(&scratch.work());
        for (name, project) in pressure_values(&config) {
            let row = entry(&payload, name);
            let source = if project.is_some() {
                Source::Project
            } else if row.user.set {
                Source::User
            } else {
                Source::Default
            };
            assert_eq!(
                row.effective.source, source,
                "{name}, rounds={rounds}, unknown={unknown}"
            );
            assert_eq!(
                row.effective.value,
                project.unwrap_or_else(|| row.user.value.clone()),
                "{name}"
            );
        }
    }
}
