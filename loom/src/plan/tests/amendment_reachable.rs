//! Integration tests for the `reachable` amendment field: the plan file, the
//! stage file and the audit log must all carry the change.

use crate::plan::amendment::{apply_amendment, AmendmentField, AmendmentPatch, AmendmentRequest};
use crate::plan::schema::ReachableCheck;
use crate::verify::transitions::{load_stage, update_stage};

use super::amendment::{audit_content, read_plan, setup_env_with_plan, snapshot_count, TestEnv};

const REACHABLE_PLAN: &str = "# PLAN: Reachable Amendment\n\n\
<!-- loom METADATA -->\n\n\
```yaml\n\
loom:\n  version: 2\n  stages:\n    - id: stage-a\n      name: \"Alpha\"\n      working_dir: \".\"\n      dependencies: []\n      acceptance:\n        - \"cargo test\"\n      contracts:\n        - id: c1\n          file: tests/c1.rs\n          test: c1\n          scenario: s\n          rejects: r\n      reachable:\n        - symbol: \"new_unit\"\n          from: \"main\"\n          description: \"impossible entry\"\n        - symbol: \"other_unit\"\n          from: \"dispatch\"\n          description: \"second entry\"\n```\n\n\
<!-- END loom METADATA -->\n";

fn check(symbol: &str, from: &str, description: &str) -> ReachableCheck {
    ReachableCheck {
        symbol: symbol.into(),
        from: from.into(),
        min_confidence: None,
        description: description.into(),
    }
}

fn setup() -> TestEnv {
    let env = setup_env_with_plan(REACHABLE_PLAN);
    update_stage("stage-a", &env.work_dir, |stage| {
        stage.reachable = vec![
            check("new_unit", "main", "impossible entry"),
            check("other_unit", "dispatch", "second entry"),
        ];
        Ok(())
    })
    .unwrap();
    env
}

fn amend(env: &TestEnv, patch: AmendmentPatch) {
    let req = AmendmentRequest {
        stage_id: "stage-a".to_string(),
        field: AmendmentField::Reachable,
        patch,
        reason: Some("resolver cannot resolve main".to_string()),
        dispute_id: None,
    };
    apply_amendment(&env.plan_path, &env.work_dir, req).unwrap();
}

#[test]
fn replace_reachable_updates_plan_stage_and_audit() {
    let env = setup();
    amend(
        &env,
        AmendmentPatch::Replace {
            index: 0,
            value: "symbol: new_unit\nfrom: dispatch\ndescription: fixed entry\n".to_string(),
        },
    );

    let plan = read_plan(&env);
    assert!(plan.contains("fixed entry"), "plan file unchanged: {plan}");
    assert!(!plan.contains("impossible entry"));

    let stage = load_stage("stage-a", &env.work_dir).unwrap();
    assert_eq!(
        stage.reachable,
        vec![
            check("new_unit", "dispatch", "fixed entry"),
            check("other_unit", "dispatch", "second entry"),
        ]
    );
    assert_eq!(snapshot_count(&env), 1);
    let log = audit_content(&env);
    assert!(log.contains("reachable"), "audit row missing field: {log}");
}

#[test]
fn delete_reachable_updates_plan_stage_and_audit() {
    let env = setup();
    amend(&env, AmendmentPatch::Delete { index: 0 });

    let plan = read_plan(&env);
    assert!(!plan.contains("impossible entry"), "plan file unchanged");
    assert!(plan.contains("second entry"));

    let stage = load_stage("stage-a", &env.work_dir).unwrap();
    assert_eq!(
        stage.reachable,
        vec![check("other_unit", "dispatch", "second entry")]
    );
    assert!(audit_content(&env).contains("reachable"));
}
