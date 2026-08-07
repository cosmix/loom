//! Tests for the per-stage `implementer` field.
//!
//! `implementer: codex` licenses a stage's session to delegate routine
//! implementation to the `codex:codex-rescue` plugin subagent instead of the
//! default Claude sonnet/haiku lane. Default is `claude`; per-stage opt-in
//! keeps the delegation choice explicit.

use super::make_stage;
use crate::plan::schema::types::{Implementer, LoomMetadata, StageType};
use crate::plan::schema::validation::validate_structural_preflight;

#[test]
fn implementer_codex_parses() {
    let yaml = r#"
loom:
  version: 1
  stages:
    - id: s1
      name: First Stage
      working_dir: "."
      artifacts: ["README.md"]
      implementer: codex
"#;
    let parsed: LoomMetadata = serde_yaml::from_str(yaml).expect("implementer: codex should parse");
    assert_eq!(parsed.loom.stages[0].implementer, Implementer::Codex);
}

#[test]
fn implementer_defaults_to_claude_when_omitted() {
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
    assert_eq!(parsed.loom.stages[0].implementer, Implementer::Claude);
}

#[test]
fn implementer_unknown_value_fails_to_parse() {
    let yaml = r#"
loom:
  version: 1
  stages:
    - id: s1
      name: First Stage
      working_dir: "."
      artifacts: ["README.md"]
      implementer: gemini
"#;
    let result: Result<LoomMetadata, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_err(),
        "an unknown implementer value should fail to parse, got: {result:?}"
    );
}

#[test]
fn implementer_codex_on_knowledge_stage_warns_in_preflight() {
    let mut stage = make_stage("explore", "Explore Codebase");
    stage.stage_type = StageType::Knowledge;
    stage.implementer = Implementer::Codex;

    let warnings = validate_structural_preflight(&[stage], None);
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("implementer") && w.contains("explore")),
        "expected an implementer advisory for the knowledge stage, got: {warnings:?}"
    );
}

#[test]
fn implementer_codex_on_standard_stage_does_not_warn() {
    let mut stage = make_stage("migrate", "Migrate Call Sites");
    stage.artifacts = vec!["README.md".to_string()];
    stage.implementer = Implementer::Codex;

    let warnings = validate_structural_preflight(&[stage], None);
    assert!(
        !warnings.iter().any(|w| w.contains("implementer")),
        "standard stages must not trigger the implementer advisory, got: {warnings:?}"
    );
}

#[test]
fn implementer_display_matches_serde_representation() {
    // conventions.md requires Display to mirror the serde form. Nothing else
    // calls Display today, so without this the two could silently diverge and
    // a log line would disagree with the value written to a stage file.
    for variant in [Implementer::Claude, Implementer::Codex] {
        let serialized = serde_yaml::to_string(&variant).expect("serialize implementer");
        assert_eq!(
            variant.to_string(),
            serialized.trim(),
            "Display for {variant:?} must match its serde representation"
        );
    }
}
