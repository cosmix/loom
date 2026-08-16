//! Regression tests for explicit `stage_type` authority.
//!
//! `stage_type` is `Option<StageType>` on `StageDefinition`: `None` means the
//! field was omitted from the plan and the id/name heuristic in
//! `detect_stage_type_from_id_name` decides; `Some(_)` is the plan author's
//! explicit, final answer and must never be second-guessed by that heuristic
//! — even when the value written is `standard` and the id/name would
//! otherwise suggest a Knowledge or IntegrationVerify stage.

use super::make_stage;
use crate::plan::schema::{detect_stage_type, StageType};

#[test]
fn explicit_standard_survives_a_knowledge_shaped_name() {
    let mut def = make_stage("catalog-stage", "Knowledge Catalog");
    def.stage_type = Some(StageType::Standard);

    assert_eq!(
        detect_stage_type(&def),
        StageType::Standard,
        "an explicit stage_type: standard must not be reclassified by the \
         id/name heuristic just because the name contains 'Knowledge'"
    );
}

#[test]
fn omitted_stage_type_falls_back_to_the_id_name_heuristic() {
    let mut def = make_stage("catalog-stage", "Knowledge Catalog");
    def.stage_type = None;

    assert_eq!(
        detect_stage_type(&def),
        StageType::Knowledge,
        "an omitted stage_type must still be classified by the id/name \
         heuristic, exactly as before this field became optional"
    );
}

#[test]
fn stage_definition_deserializes_with_stage_type_omitted() {
    // `StageDefinition` is `#[serde(deny_unknown_fields)]`; `Option<StageType>`
    // with `#[serde(default)]` must still accept a wholly absent key (the
    // overwhelmingly common case — most plans never write `stage_type` at
    // all). A wrong attribute here would fail every existing plan to parse.
    let yaml = r#"
loom:
  version: 1
  stages:
    - id: stage-1
      name: Stage One
      working_dir: "."
      acceptance:
        - "true"
"#;
    let metadata: crate::plan::schema::LoomMetadata = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(metadata.loom.stages[0].stage_type, None);
}
