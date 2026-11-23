//! Contract: a `..` inside a file name is not a parent component; only a
//! `..` path component counts as path traversal.

use crate::plan::schema::{validate, LoomConfig, LoomMetadata, StageDefinition, WiringCheck};

fn wiring(source: &str) -> WiringCheck {
    WiringCheck {
        source: source.to_string(),
        pattern: "fn main".to_string(),
        description: "d".to_string(),
        literal: true,
    }
}

fn stage(working_dir: &str, artifact: &str, wiring_source: &str) -> StageDefinition {
    StageDefinition {
        id: "s1".to_string(),
        name: "Stage One".to_string(),
        working_dir: working_dir.to_string(),
        artifacts: vec![artifact.to_string()],
        wiring: vec![wiring(wiring_source)],
        ..Default::default()
    }
}

/// Every validation message for a one-stage plan holding `stage`.
fn messages(stage: StageDefinition) -> Vec<String> {
    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            stages: vec![stage],
            ..Default::default()
        },
    };
    match validate(&metadata) {
        Ok(()) => Vec::new(),
        Err(errors) => errors.iter().map(ToString::to_string).collect(),
    }
}

fn traversal(messages: &[String]) -> bool {
    messages.iter().any(|m| m.contains("path traversal"))
}

#[test]
fn dotdot_inside_a_name_is_accepted_and_a_parent_component_refused() {
    let accepted = messages(stage(".", "out/a..b.txt", "src/foo..bar.rs"));
    assert!(!traversal(&accepted), "names with .. refused: {accepted:?}");

    let parent_source = messages(stage(".", "out/a..b.txt", "../x"));
    assert!(traversal(&parent_source), "../x source: {parent_source:?}");

    let parent_artifact = messages(stage(".", "a/../b", "src/foo..bar.rs"));
    assert!(traversal(&parent_artifact), "a/../b: {parent_artifact:?}");

    let parent_dir = messages(stage("../x", "out/a..b.txt", "src/foo..bar.rs"));
    assert!(traversal(&parent_dir), "../x working_dir: {parent_dir:?}");
}
