//! Tests for the per-stage `subagent_timeout_secs` field.
//!
//! The field bounds how long a stage may go without a heartbeat before the
//! orchestrator flags it as silent. Omitted means the built-in
//! `DEFAULT_HUNG_TIMEOUT_SECS`, so every plan written before the field existed
//! keeps exactly its old behaviour. Propagation from definition to stage model
//! is covered alongside `ultracode`/`implementer` in `commands::init::tests`.

use crate::models::stage::Stage;
use crate::orchestrator::monitor::heartbeat::DEFAULT_HUNG_TIMEOUT_SECS;
use crate::plan::schema::types::LoomMetadata;

#[test]
fn subagent_timeout_secs_parses() {
    let yaml = r#"
loom:
  version: 1
  stages:
    - id: s1
      name: First Stage
      working_dir: "."
      artifacts: ["README.md"]
      subagent_timeout_secs: 900
"#;
    let parsed: LoomMetadata =
        serde_yaml::from_str(yaml).expect("subagent_timeout_secs should parse");
    assert_eq!(parsed.loom.stages[0].subagent_timeout_secs, Some(900));
}

#[test]
fn subagent_timeout_backwards_compat_omitted_falls_back_to_default() {
    // A plan written before this field existed must still parse, and the stage
    // it yields must be measured against exactly the old hardcoded threshold.
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
    assert_eq!(parsed.loom.stages[0].subagent_timeout_secs, None);

    let stage = Stage {
        subagent_timeout_secs: parsed.loom.stages[0].subagent_timeout_secs,
        ..Default::default()
    };
    assert_eq!(
        stage.effective_subagent_timeout_secs(),
        DEFAULT_HUNG_TIMEOUT_SECS
    );
}

#[test]
fn effective_subagent_timeout_prefers_the_explicit_value() {
    let mut stage = Stage::default();
    assert_eq!(
        stage.effective_subagent_timeout_secs(),
        DEFAULT_HUNG_TIMEOUT_SECS,
        "an unset budget must fall back to the built-in default"
    );

    stage.subagent_timeout_secs = Some(1200);
    assert_eq!(
        stage.effective_subagent_timeout_secs(),
        1200,
        "an explicit budget must win over the default"
    );
}
