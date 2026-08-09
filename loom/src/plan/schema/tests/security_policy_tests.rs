//! Fail-closed plan policy regression tests.

use super::create_valid_metadata;
use crate::plan::schema::types::LoomMetadata;
use crate::plan::schema::unsafe_plan_reasons;
use crate::plan::schema::validation::validate;

fn parse(yaml: &str) -> Result<LoomMetadata, serde_yaml::Error> {
    serde_yaml::from_str(yaml)
}

const UNKNOWN_NESTED_FIELD_CASES: [(&str, &str, &str); 6] = [
    (
        "WiringCheck",
        "pattrn",
        r#"      wiring:
        - source: src/lib.rs
          pattern: mod_api
          description: API module is wired
          pattrn: typo
"#,
    ),
    (
        "TruthCheck",
        "stdout_contians",
        r#"      before_stage:
        - command: cargo test
          stdout_contains: [passed]
          stdout_contians: [typo]
"#,
    ),
    (
        "SuccessCriteria",
        "stderr_emty",
        r#"      wiring_tests:
        - name: CLI smoke
          command: loom --help
          success_criteria:
            exit_code: 0
            stderr_emty: true
"#,
    ),
    (
        "WiringTest",
        "commnad",
        r#"      wiring_tests:
        - name: CLI smoke
          command: loom --help
          commnad: loom status
"#,
    ),
    (
        "DeadCodeCheck",
        "fail_pattrns",
        r#"      dead_code_check:
        command: cargo check
        fail_patterns: [unused]
        fail_pattrns: [typo]
"#,
    ),
    (
        "RegressionTest",
        "must_contian",
        r#"      regression_test:
        file: tests/regression.rs
        must_contain: [test_bug]
        must_contian: [typo]
"#,
    ),
];

const VALID_NESTED_SCHEMA_PLAN: &str = r#"
loom:
  version: 1
  stages:
    - id: strict-stage
      name: Strict Stage
      working_dir: "."
      acceptance:
        - cargo test
      wiring:
        - source: src/lib.rs
          pattern: mod_api
          description: API module is wired
      wiring_tests:
        - name: CLI smoke
          command: loom --help
          success_criteria:
            exit_code: 0
            stdout_contains: [Usage]
            stdout_not_contains: [panic]
            stderr_contains: []
            stderr_empty: false
          description: CLI starts successfully
      dead_code_check:
        command: cargo check
        fail_patterns: [unused]
        ignore_patterns: [allowed_unused]
      before_stage:
        - command: cargo test
          stdout_contains: [passed]
          stdout_not_contains: [failed]
          stderr_empty: true
          exit_code: 0
          description: Baseline passes
      bug_fix: true
      regression_test:
        file: tests/regression.rs
        must_contain: [test_bug]
"#;

fn assert_unknown_nested_field_rejected(type_name: &str, field: &str, stage_fields: &str) {
    let yaml = format!(
        "loom:\n  version: 1\n  stages:\n    - id: strict-stage\n      name: Strict Stage\n      working_dir: \".\"\n{stage_fields}"
    );
    let error = parse(&yaml).unwrap_err().to_string();
    assert!(
        error.contains(&format!("unknown field `{field}`")),
        "{type_name} accepted `{field}` or returned the wrong error: {error}"
    );
}

#[test]
fn rejects_unknown_fields_in_nested_runtime_schema_types() {
    for (type_name, field, stage_fields) in UNKNOWN_NESTED_FIELD_CASES {
        assert_unknown_nested_field_rejected(type_name, field, stage_fields);
    }
}

#[test]
fn accepts_representative_valid_nested_runtime_schema_data() {
    let metadata = parse(VALID_NESTED_SCHEMA_PLAN).unwrap();
    let stage = &metadata.loom.stages[0];

    assert_eq!(stage.wiring.len(), 1);
    assert_eq!(stage.before_stage.len(), 1);
    assert_eq!(stage.wiring_tests.len(), 1);
    assert_eq!(stage.wiring_tests[0].success_criteria.exit_code, Some(0));
    assert!(stage.dead_code_check.is_some());
    assert!(stage.regression_test.is_some());
}

#[test]
fn rejects_misspelled_stage_type_instead_of_defaulting() {
    let error = parse(
        r#"
loom:
  version: 1
  stages:
    - id: stage-1
      name: Stage One
      working_dir: "."
      stage-type: integration-verify
"#,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("unknown field `stage-type`"), "{error}");
}

#[test]
fn rejects_misspelled_plan_sandbox_field() {
    let error = parse(
        r#"
loom:
  version: 1
  sandbox:
    enabeld: false
  stages: []
"#,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("unknown field `enabeld`"), "{error}");
}

#[test]
fn rejects_misspelled_nested_sandbox_fields() {
    let cases = [
        (
            "deny_reed",
            r#"
loom:
  version: 1
  sandbox:
    filesystem:
      deny_reed: ["~/.ssh/**"]
  stages: []
"#,
        ),
        (
            "allow_local_bindng",
            r#"
loom:
  version: 1
  sandbox:
    network:
      allow_local_bindng: true
  stages: []
"#,
        ),
        (
            "enable_weaker_nested_typo",
            r#"
loom:
  version: 1
  sandbox:
    linux:
      enable_weaker_nested_typo: true
  stages: []
"#,
        ),
        (
            "enabeld",
            r#"
loom:
  version: 1
  stages:
    - id: stage-1
      name: Stage One
      working_dir: "."
      sandbox:
        enabeld: false
"#,
        ),
    ];

    for (field, yaml) in cases {
        let error = parse(yaml).unwrap_err().to_string();
        assert!(
            error.contains(&format!("unknown field `{field}`")),
            "{error}"
        );
    }
}

#[test]
fn rejects_true_unix_socket_shorthand() {
    let error = parse(
        r#"
loom:
  version: 1
  sandbox:
    network:
      allow_unix_sockets: true
  stages: []
"#,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("allow_all_unix_sockets"), "{error}");
}

#[test]
fn accepts_false_unix_socket_shorthand() {
    let metadata = parse(
        r#"
loom:
  version: 1
  sandbox:
    network:
      allow_unix_sockets: false
  stages: []
"#,
    )
    .unwrap();
    assert!(metadata.loom.sandbox.network.allow_unix_sockets.is_empty());
}

#[test]
fn rejects_command_exclusions_instead_of_expanding_them() {
    let mut metadata = create_valid_metadata();
    metadata.loom.sandbox.excluded_commands = vec!["git:*".to_string()];
    let errors = validate(&metadata).unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.message.contains("excluded_commands")));
}

#[test]
fn unsafe_plan_reasons_cover_plan_and_stage_overrides() {
    let mut metadata = create_valid_metadata();
    metadata.loom.sandbox.enabled = false;
    metadata.loom.sandbox.allow_unsandboxed_escape = true;
    metadata.loom.stages[0].sandbox.enabled = Some(false);

    let reasons = unsafe_plan_reasons(&metadata);
    assert!(reasons
        .iter()
        .any(|reason| reason.contains("plan sandbox.enabled")));
    assert!(reasons
        .iter()
        .any(|reason| reason.contains("allow_unsandboxed_escape")));
    assert!(reasons.iter().any(|reason| reason.contains("stage-1")));
}
