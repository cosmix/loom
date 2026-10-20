//! Tests for the `model` serde validator.

use crate::claude::CLAUDE_MODELS;
use crate::plan::schema::types::LoomMetadata;

const VALID_PLAN_TEMPLATE: &str = r#"
loom:
  version: 1
  stages:
    - id: s1
      name: First Stage
      working_dir: "."
      artifacts: ["README.md"]
      model: "{model}"
"#;

const PLAN_WITHOUT_MODEL_TEMPLATE: &str = r#"
loom:
  version: 1
  stages:
    - id: s1
      name: First Stage
      working_dir: "."
      artifacts: ["README.md"]
"#;

fn plan_yaml_with_model(model: &str) -> String {
    VALID_PLAN_TEMPLATE.replace("{model}", model)
}

#[test]
fn accepts_each_allowed_model() {
    for model in CLAUDE_MODELS {
        let yaml = plan_yaml_with_model(model);
        let parsed: LoomMetadata = crate::yaml::from_str(&yaml).expect("allowed model must parse");
        assert_eq!(parsed.loom.stages[0].model.as_deref(), Some(*model));
    }
}

#[test]
fn rejects_invalid_models() {
    for model in ["claude-opus-5", "opus[1m]", "opus; rm -rf /", ""] {
        let yaml = plan_yaml_with_model(model);
        let err = crate::yaml::from_str::<LoomMetadata>(&yaml)
            .expect_err("invalid model must be rejected");
        assert!(format!("{err}").contains("invalid model"));
    }
}

#[test]
fn missing_model_parses_as_none() {
    let parsed: LoomMetadata = crate::yaml::from_str(PLAN_WITHOUT_MODEL_TEMPLATE)
        .expect("plan without a model must parse");
    assert!(parsed.loom.stages[0].model.is_none());
}
