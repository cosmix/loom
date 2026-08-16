//! Tests for the per-stage `implementers` field.
//!
//! `implementers` names the agent lanes a stage may spawn subagents from, in
//! preference order — the first is what routine implementation reaches for.
//! Listing a lane makes it AVAILABLE, never mandatory: a stage mixes lanes
//! freely, choosing per subagent by what the work needs. Default is
//! `["claude"]`, since the Claude lane is the harness the session already runs
//! in; codex needs an explicit opt-in because it needs a plugin and carries its
//! own safety doctrine.

use super::make_stage;
use crate::plan::schema::types::{Implementer, Implementers, LoomMetadata, StageType};
use crate::plan::schema::validation::{validate, validate_structural_preflight};

#[test]
fn implementers_codex_only_parses() {
    let yaml = r#"
loom:
  version: 1
  stages:
    - id: s1
      name: First Stage
      working_dir: "."
      artifacts: ["README.md"]
      implementers: ["codex"]
"#;
    let parsed: LoomMetadata = serde_yaml::from_str(yaml).expect("implementers: [codex] parses");
    let lanes = &parsed.loom.stages[0].implementers;
    assert_eq!(lanes.preferred(), Implementer::Codex);
    assert!(lanes.includes_codex());
    assert!(!lanes.includes_claude());
    assert!(!lanes.is_mixed());
}

#[test]
fn implementers_mixed_parses_and_preserves_preference_order() {
    let yaml = r#"
loom:
  version: 1
  stages:
    - id: s1
      name: First Stage
      working_dir: "."
      artifacts: ["README.md"]
      implementers: ["codex", "claude"]
"#;
    let parsed: LoomMetadata = serde_yaml::from_str(yaml).expect("mixed implementers parse");
    let lanes = &parsed.loom.stages[0].implementers;
    assert!(lanes.is_mixed());
    assert!(lanes.includes_codex());
    assert!(lanes.includes_claude());
    assert_eq!(
        lanes.preferred(),
        Implementer::Codex,
        "the FIRST listed lane is the one routine implementation prefers"
    );
}

#[test]
fn implementers_preference_order_is_not_sorted_or_normalized() {
    // The inverse of the previous case: the same two lanes in the other order
    // must yield the other preference. Guards against a future normalization
    // (dedup via a set, a sort, a canonical ordering) silently discarding the
    // one piece of information the ORDER carries.
    let yaml = r#"
loom:
  version: 1
  stages:
    - id: s1
      name: First Stage
      working_dir: "."
      artifacts: ["README.md"]
      implementers: ["claude", "codex"]
"#;
    let parsed: LoomMetadata = serde_yaml::from_str(yaml).expect("mixed implementers parse");
    let lanes = &parsed.loom.stages[0].implementers;
    assert_eq!(lanes.preferred(), Implementer::Claude);
    assert!(
        lanes.includes_codex(),
        "codex is licensed even when it is not preferred"
    );
}

#[test]
fn implementers_defaults_to_claude_when_omitted() {
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
    let lanes = &parsed.loom.stages[0].implementers;
    assert_eq!(lanes.preferred(), Implementer::Claude);
    assert!(!lanes.includes_codex());
    assert!(!lanes.is_mixed());
}

#[test]
fn implementers_unknown_lane_fails_to_parse() {
    let yaml = r#"
loom:
  version: 1
  stages:
    - id: s1
      name: First Stage
      working_dir: "."
      artifacts: ["README.md"]
      implementers: ["gemini"]
"#;
    let result: Result<LoomMetadata, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_err(),
        "an unknown lane should fail to parse, got: {result:?}"
    );
}

#[test]
fn implementers_scalar_form_fails_to_parse() {
    // The field is a list. A bare scalar is the shape a plan author reaches for
    // out of habit, and silently accepting it would reintroduce the assumption
    // that a stage has exactly one lane.
    let yaml = r#"
loom:
  version: 1
  stages:
    - id: s1
      name: First Stage
      working_dir: "."
      artifacts: ["README.md"]
      implementers: codex
"#;
    let result: Result<LoomMetadata, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_err(),
        "a scalar implementers value should fail to parse, got: {result:?}"
    );
}

#[test]
fn empty_implementers_list_is_a_validation_error() {
    let yaml = r#"
loom:
  version: 1
  stages:
    - id: s1
      name: First Stage
      working_dir: "."
      artifacts: ["README.md"]
      implementers: []
"#;
    let parsed: LoomMetadata = serde_yaml::from_str(yaml).expect("empty list parses");
    let errors = validate(&parsed).expect_err("an empty lane list must fail validation");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("implementers") && e.stage_id.as_deref() == Some("s1")),
        "expected an implementers error for s1, got: {errors:?}"
    );
}

#[test]
fn duplicate_lane_is_a_validation_error() {
    let yaml = r#"
loom:
  version: 1
  stages:
    - id: s1
      name: First Stage
      working_dir: "."
      artifacts: ["README.md"]
      implementers: ["codex", "claude", "codex"]
"#;
    let parsed: LoomMetadata = serde_yaml::from_str(yaml).expect("duplicate list parses");
    let errors = validate(&parsed).expect_err("a duplicated lane must fail validation");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("more than once") && e.message.contains("codex")),
        "expected a duplicate-lane error naming codex, got: {errors:?}"
    );
}

#[test]
fn codex_lane_on_knowledge_stage_warns_in_preflight() {
    let mut stage = make_stage("explore", "Explore Codebase");
    stage.stage_type = Some(StageType::Knowledge);
    stage.implementers = Implementers::new(vec![Implementer::Codex]);

    let warnings = validate_structural_preflight(&[stage], None);
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("implementers") && w.contains("explore")),
        "expected an implementers advisory for the knowledge stage, got: {warnings:?}"
    );
}

#[test]
fn codex_warns_on_knowledge_stage_even_when_only_secondary() {
    // The advisory is about the lane being AVAILABLE on a stage whose work is
    // curation, not about which lane is preferred — a knowledge stage that
    // merely lists codex can still misroute its work to it.
    let mut stage = make_stage("explore", "Explore Codebase");
    stage.stage_type = Some(StageType::Knowledge);
    stage.implementers = Implementers::new(vec![Implementer::Claude, Implementer::Codex]);

    let warnings = validate_structural_preflight(&[stage], None);
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("implementers") && w.contains("explore")),
        "expected the advisory for a codex-secondary knowledge stage, got: {warnings:?}"
    );
}

#[test]
fn codex_lane_on_standard_stage_does_not_warn() {
    let mut stage = make_stage("migrate", "Migrate Call Sites");
    stage.artifacts = vec!["README.md".to_string()];
    stage.implementers = Implementers::new(vec![Implementer::Codex, Implementer::Claude]);

    let warnings = validate_structural_preflight(&[stage], None);
    assert!(
        !warnings.iter().any(|w| w.contains("implementers")),
        "standard stages must not trigger the advisory, got: {warnings:?}"
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

#[test]
fn implementers_display_lists_lanes_in_order() {
    let lanes = Implementers::new(vec![Implementer::Codex, Implementer::Claude]);
    assert_eq!(lanes.to_string(), "codex, claude");
}
