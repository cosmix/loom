//! Contract: schema validation refuses a dependency cycle of any length.

use crate::plan::parser::parse_and_validate;
use crate::plan::schema::{validate, LoomConfig, LoomMetadata, StageDefinition, WiringCheck};

fn stage(id: &str, dependencies: &[&str]) -> StageDefinition {
    StageDefinition {
        id: id.to_string(),
        name: format!("Stage {id}"),
        working_dir: ".".to_string(),
        dependencies: dependencies.iter().map(|d| d.to_string()).collect(),
        artifacts: vec!["README.md".to_string()],
        wiring: vec![WiringCheck {
            source: "README.md".to_string(),
            pattern: "readme".to_string(),
            description: "the readme is present".to_string(),
            literal: false,
        }],
        ..Default::default()
    }
}

fn plan(stages: Vec<StageDefinition>) -> LoomMetadata {
    LoomMetadata {
        loom: LoomConfig {
            version: 1,
            stages,
            ..Default::default()
        },
    }
}

#[test]
fn three_stage_cycle_fails_validation_naming_it() {
    let cyclic = plan(vec![
        stage("a", &["c"]),
        stage("b", &["a"]),
        stage("c", &["b"]),
    ]);

    let errors = validate(&cyclic).expect_err("a three-stage cycle must fail validation");
    let messages: Vec<String> = errors.iter().map(ToString::to_string).collect();
    let cycle = messages
        .iter()
        .find(|m| m.contains("Circular dependency detected"))
        .unwrap_or_else(|| panic!("no cycle error in {messages:?}"));
    for id in ["a", "b", "c"] {
        assert!(
            cycle.contains(id),
            "cycle error does not name {id}: {cycle}"
        );
    }

    let yaml = crate::yaml::to_string(&cyclic).unwrap();
    let parsed = parse_and_validate(&yaml).expect_err("parse_and_validate must refuse the cycle");
    let text = format!("{parsed:#}");
    assert!(text.contains("Circular dependency detected"), "{text}");

    let acyclic = plan(vec![
        stage("a", &[]),
        stage("b", &["a"]),
        stage("c", &["a", "b"]),
    ]);
    assert!(validate(&acyclic).is_ok(), "{:?}", validate(&acyclic).err());
}
