//! Integration tests for the field-scoped Case 2 catch-up performed by
//! `verify_plan_versions_consistency` (see the `amendment_catch_up` module).
//!
//! Three scenarios: (1) the amended field is already on disk, so any other
//! edit made to the plan afterward must survive untouched; (2) the amended
//! field never reached the live plan (crash between snapshot and write),
//! while an unrelated edit landed anyway — both must be reconciled
//! correctly; (3) the live plan is unparseable, so nothing may be written.

use std::fs;

use crate::plan::amendment::{
    apply_amendment, verify_plan_versions_consistency, AmendmentField, AmendmentPatch,
    AmendmentRequest,
};

use super::amendment::{make_acceptance_yaml, read_plan, setup_env_with_plan, TestEnv};

const TWO_STAGE_PLAN: &str = "# PLAN: Amendment Catch-up Test\n\n\
Human-readable section with **prose**.\n\n\
<!-- loom METADATA -->\n\n\
```yaml\n\
loom:\n  version: 1\n  adjudication:\n    max_amendments_per_stage: 3\n  stages:\n    - id: stage-a\n      name: \"Alpha\"\n      working_dir: \".\"\n      dependencies: []\n      acceptance:\n        - \"cargo test\"\n        - \"cargo clippy\"\n      wiring:\n        - source: \"src/lib.rs\"\n          pattern: \"pub fn foo\"\n          description: \"foo is exported\"\n      wiring_tests:\n        - name: \"smoke test\"\n          command: \"true\"\n    - id: stage-b\n      name: \"Beta\"\n      description: \"original description\"\n      working_dir: \".\"\n      dependencies: []\n      acceptance:\n        - \"cargo check\"\n```\n\n\
<!-- END loom METADATA -->\n\n\
Trailing prose paragraph.\n";

fn amend_stage_a(env: &TestEnv, new_value: &str) {
    let req = AmendmentRequest {
        stage_id: "stage-a".to_string(),
        field: AmendmentField::Acceptance,
        patch: AmendmentPatch::Replace {
            index: 0,
            value: make_acceptance_yaml(new_value),
        },
        reason: None,
        dispute_id: None,
    };
    apply_amendment(&env.plan_path, &env.work_dir, req).unwrap();
}

/// Apply a legitimate, amendment-unrelated edit: change stage-b's
/// description and append a new prose paragraph.
fn with_unrelated_edit(content: &str) -> String {
    let mut edited = content.replacen("original description", "updated description", 1);
    edited.push_str("\n\nAn additional prose paragraph added after the amendment.\n");
    edited
}

/// Replace the YAML metadata body with text that cannot parse, while
/// leaving the surrounding fence markers intact.
fn corrupt_yaml_metadata(content: &str) -> String {
    let fence_open = "```yaml\n";
    let start = content.find(fence_open).unwrap() + fence_open.len();
    let end = start + content[start..].find("\n```").unwrap();
    format!(
        "{}loom: {{version: 1, stages: [unterminated{}",
        &content[..start],
        &content[end..]
    )
}

#[test]
fn unrelated_edits_survive_when_amended_field_already_matches() {
    let env = setup_env_with_plan(TWO_STAGE_PLAN);
    amend_stage_a(&env, "cargo test --release");

    let edited = with_unrelated_edit(&read_plan(&env));
    fs::write(&env.plan_path, &edited).unwrap();

    let actions = verify_plan_versions_consistency(&env.plan_path, &env.work_dir).unwrap();
    assert_eq!(
        actions, 0,
        "amended field already on disk — nothing to catch up"
    );
    assert_eq!(
        read_plan(&env),
        edited,
        "unrelated edits must survive byte-for-byte"
    );
}

#[test]
fn crash_between_snapshot_and_write_is_reconciled_without_losing_unrelated_edit() {
    let env = setup_env_with_plan(TWO_STAGE_PLAN);
    amend_stage_a(&env, "cargo test --from-snapshot");

    // Simulate the crash: the live plan never received the amendment, but a
    // legitimate edit landed on it anyway (e.g. another commit).
    let stale = with_unrelated_edit(TWO_STAGE_PLAN);
    fs::write(&env.plan_path, &stale).unwrap();

    let actions = verify_plan_versions_consistency(&env.plan_path, &env.work_dir).unwrap();
    assert_eq!(actions, 1, "exactly the amended field needed reconciling");

    let plan = read_plan(&env);
    assert!(
        plan.contains("cargo test --from-snapshot"),
        "amended field must now match the snapshot: {plan}"
    );
    assert!(
        plan.contains("updated description"),
        "unrelated edit must survive: {plan}"
    );
    assert!(
        plan.contains("An additional prose paragraph added after the amendment."),
        "unrelated prose edit must survive: {plan}"
    );
}

#[test]
fn unparseable_live_plan_is_never_written() {
    let env = setup_env_with_plan(TWO_STAGE_PLAN);
    amend_stage_a(&env, "cargo test --release");

    let corrupted = corrupt_yaml_metadata(&read_plan(&env));
    fs::write(&env.plan_path, &corrupted).unwrap();
    let before = fs::read(&env.plan_path).unwrap();

    let actions = verify_plan_versions_consistency(&env.plan_path, &env.work_dir).unwrap();
    assert_eq!(
        actions, 0,
        "an unparseable live plan must not be reconciled"
    );
    assert_eq!(
        fs::read(&env.plan_path).unwrap(),
        before,
        "live plan bytes must be untouched"
    );
}
