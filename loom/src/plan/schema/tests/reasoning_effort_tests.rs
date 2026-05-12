//! Tests for the `reasoning_effort` serde validator.
//!
//! See [`crate::plan::schema::types::deserialize_reasoning_effort`] — the
//! validator rejects any value outside the canonical set so that a
//! malicious plan cannot smuggle shell metacharacters into the
//! `--effort <value>` argument that `native/mod.rs` appends to the Claude
//! Code command line.

use crate::plan::schema::types::{LoomMetadata, ALLOWED_REASONING_EFFORTS};

const VALID_PLAN_TEMPLATE: &str = r#"
loom:
  version: 1
  stages:
    - id: s1
      name: First Stage
      working_dir: "."
      artifacts: ["README.md"]
      reasoning_effort: "{effort}"
"#;

fn plan_yaml_with_effort(effort: &str) -> String {
    VALID_PLAN_TEMPLATE.replace("{effort}", effort)
}

#[test]
fn accepts_each_allowed_value() {
    for effort in ALLOWED_REASONING_EFFORTS {
        let yaml = plan_yaml_with_effort(effort);
        let parsed: Result<LoomMetadata, _> = serde_yaml::from_str(&yaml);
        assert!(
            parsed.is_ok(),
            "expected '{effort}' to parse, got: {:?}",
            parsed.err()
        );
        let parsed = parsed.unwrap();
        assert_eq!(
            parsed.loom.stages[0].reasoning_effort.as_deref(),
            Some(*effort)
        );
    }
}

#[test]
fn rejects_shell_injection_attempt() {
    // Codex-noted attack vector: malicious plan smuggles shell metacharacters.
    let yaml = plan_yaml_with_effort("low; rm -rf / #");
    let err = serde_yaml::from_str::<LoomMetadata>(&yaml)
        .expect_err("malicious reasoning_effort must be rejected");
    let msg = format!("{err}");
    assert!(
        msg.contains("invalid reasoning_effort"),
        "error must mention the invalid field, got: {msg}"
    );
    assert!(
        msg.contains("low; rm -rf / #"),
        "error must echo the invalid value, got: {msg}"
    );
    assert!(
        msg.contains("Allowed values"),
        "error must list allowed values, got: {msg}"
    );
}

#[test]
fn rejects_random_string() {
    let yaml = plan_yaml_with_effort("ultra-mega");
    let err = serde_yaml::from_str::<LoomMetadata>(&yaml).expect_err("should reject 'ultra-mega'");
    assert!(format!("{err}").contains("invalid reasoning_effort"));
}

#[test]
fn rejects_uppercase_variant() {
    // Anchored to lowercase; Claude Code CLI is case-sensitive.
    let yaml = plan_yaml_with_effort("HIGH");
    let err = serde_yaml::from_str::<LoomMetadata>(&yaml).expect_err("uppercase rejected");
    assert!(format!("{err}").contains("invalid reasoning_effort"));
}

#[test]
fn missing_field_parses_as_none() {
    let yaml = r#"
loom:
  version: 1
  stages:
    - id: s1
      name: First Stage
      working_dir: "."
      artifacts: ["README.md"]
"#;
    let parsed: LoomMetadata = serde_yaml::from_str(yaml).expect("should parse without effort");
    assert!(parsed.loom.stages[0].reasoning_effort.is_none());
}

#[test]
fn null_field_parses_as_none() {
    let yaml = r#"
loom:
  version: 1
  stages:
    - id: s1
      name: First Stage
      working_dir: "."
      artifacts: ["README.md"]
      reasoning_effort: null
"#;
    let parsed: LoomMetadata = serde_yaml::from_str(yaml).expect("null should be accepted");
    assert!(parsed.loom.stages[0].reasoning_effort.is_none());
}
