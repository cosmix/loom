//! Tests for the per-stage `ultracode` flag.
//!
//! `ultracode: true` licenses a stage's session for Workflow orchestration
//! (the spawn prompt carries the literal keyword and the signal gains an
//! "## Ultracode Mode" section). Default is false; per-stage opt-in keeps
//! the cost decision explicit.

use super::make_stage;
use crate::plan::schema::types::{LoomMetadata, StageType};
use crate::plan::schema::validation::validate_structural_preflight;

#[test]
fn ultracode_true_parses() {
    let yaml = r#"
loom:
  version: 1
  stages:
    - id: s1
      name: First Stage
      working_dir: "."
      artifacts: ["README.md"]
      ultracode: true
"#;
    let parsed: LoomMetadata = serde_yaml::from_str(yaml).expect("ultracode: true should parse");
    assert!(parsed.loom.stages[0].ultracode);
}

#[test]
fn ultracode_defaults_to_false_when_omitted() {
    let yaml = r#"
loom:
  version: 1
  stages:
    - id: s1
      name: First Stage
      working_dir: "."
      artifacts: ["README.md"]
"#;
    let parsed: LoomMetadata = serde_yaml::from_str(yaml).expect("plan should parse");
    assert!(!parsed.loom.stages[0].ultracode);
}

#[test]
fn ultracode_on_knowledge_stage_warns_in_preflight() {
    let mut stage = make_stage("explore", "Explore Codebase");
    stage.stage_type = StageType::Knowledge;
    stage.ultracode = true;

    let warnings = validate_structural_preflight(&[stage], None);
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("ultracode") && w.contains("explore")),
        "expected an ultracode advisory for the knowledge stage, got: {warnings:?}"
    );
}

#[test]
fn ultracode_on_standard_stage_does_not_warn() {
    let mut stage = make_stage("migrate", "Migrate Call Sites");
    stage.artifacts = vec!["README.md".to_string()];
    stage.ultracode = true;

    let warnings = validate_structural_preflight(&[stage], None);
    assert!(
        !warnings.iter().any(|w| w.contains("ultracode")),
        "standard stages must not trigger the ultracode advisory, got: {warnings:?}"
    );
}
