//! Integration tests for the `wiring_tests` amendment field, plus one
//! `acceptance` test (1) moved here to keep `plan::tests::amendment` under
//! its maintainability line budget.
//!
//! Split out of `plan::tests::amendment` to keep both files under the
//! maintainability line limits. Shared fixtures (`PLAN_CONTENT`, `setup_env`,
//! `make_stage`, `read_plan`, `audit_content`, `snapshot_count`,
//! `make_acceptance_yaml`, `make_wiring_test_yaml`) live there and are
//! imported here.

use crate::plan::amendment::{
    apply_amendment, verify_plan_versions_consistency, AmendmentField, AmendmentPatch,
    AmendmentRequest,
};
use crate::verify::transitions::{load_stage, update_stage};

use super::amendment::{
    audit_content, make_acceptance_yaml, make_stage, make_wiring_test_yaml, read_plan, setup_env,
    snapshot_count, PLAN_CONTENT,
};

// --------------------------------------------------------------------------
// 1. Replace acceptance
// --------------------------------------------------------------------------
#[test]
fn replace_acceptance_succeeds_and_updates_plan_and_stage() {
    let env = setup_env();
    let req = AmendmentRequest {
        stage_id: "stage-a".to_string(),
        field: AmendmentField::Acceptance,
        patch: AmendmentPatch::Replace {
            index: 0,
            value: make_acceptance_yaml("cargo test --release"),
        },
        reason: Some("env mismatch".to_string()),
        dispute_id: Some("d-1".to_string()),
    };
    let result = apply_amendment(&env.plan_path, &env.work_dir, req).unwrap();
    assert_eq!(result.version, 1);
    assert_eq!(result.amendments_applied, 1);

    // Plan file was updated.
    let new_plan = read_plan(&env);
    assert!(new_plan.contains("cargo test --release"));

    // Stage file was updated.
    let stage = load_stage("stage-a", &env.work_dir).unwrap();
    assert_eq!(stage.acceptance[0].command(), "cargo test --release");
    assert_eq!(stage.acceptance[1].command(), "cargo clippy");

    // Snapshot exists.
    assert_eq!(snapshot_count(&env), 1);
}

// --------------------------------------------------------------------------
// 6b. Replace wiring_tests
// --------------------------------------------------------------------------
#[test]
fn replace_wiring_tests_succeeds_and_updates_plan_and_stage() {
    let env = setup_env();
    let req = AmendmentRequest {
        stage_id: "stage-a".to_string(),
        field: AmendmentField::WiringTests,
        patch: AmendmentPatch::Replace {
            index: 0,
            value: make_wiring_test_yaml("smoke test v2", "true"),
        },
        reason: Some("flaky smoke test".to_string()),
        dispute_id: Some("d-9".to_string()),
    };
    apply_amendment(&env.plan_path, &env.work_dir, req).unwrap();

    // Plan file was updated.
    let new_plan = read_plan(&env);
    assert!(new_plan.contains("smoke test v2"));

    // Stage file was updated.
    let stage = load_stage("stage-a", &env.work_dir).unwrap();
    assert_eq!(stage.wiring_tests.len(), 1);
    assert_eq!(stage.wiring_tests[0].name, "smoke test v2");
    assert_eq!(stage.wiring_tests[0].command, "true");

    // Audit row records the field name.
    let log = audit_content(&env);
    assert!(
        log.contains("wiring_tests"),
        "audit row must record the field name: {log}"
    );
}

// --------------------------------------------------------------------------
// 6c. Insert wiring_tests
// --------------------------------------------------------------------------
#[test]
fn insert_wiring_tests_extends_list() {
    let env = setup_env();
    let req = AmendmentRequest {
        stage_id: "stage-a".to_string(),
        field: AmendmentField::WiringTests,
        patch: AmendmentPatch::Insert {
            index: 1,
            value: make_wiring_test_yaml("second test", "false"),
        },
        reason: None,
        dispute_id: None,
    };
    apply_amendment(&env.plan_path, &env.work_dir, req).unwrap();
    let stage = load_stage("stage-a", &env.work_dir).unwrap();
    assert_eq!(stage.wiring_tests.len(), 2);
    assert_eq!(stage.wiring_tests[1].name, "second test");
}

// --------------------------------------------------------------------------
// 6d. Delete wiring_tests
// --------------------------------------------------------------------------
#[test]
fn delete_wiring_tests_removes_entry() {
    let env = setup_env();
    let req = AmendmentRequest {
        stage_id: "stage-a".to_string(),
        field: AmendmentField::WiringTests,
        patch: AmendmentPatch::Delete { index: 0 },
        reason: None,
        dispute_id: None,
    };
    apply_amendment(&env.plan_path, &env.work_dir, req).unwrap();
    let stage = load_stage("stage-a", &env.work_dir).unwrap();
    assert!(stage.wiring_tests.is_empty());
}

// --------------------------------------------------------------------------
// 6e. Out-of-bounds index on wiring_tests errors before any I/O
// --------------------------------------------------------------------------
#[test]
fn wiring_tests_out_of_bounds_index_errors_before_any_io() {
    let env = setup_env();
    let req = AmendmentRequest {
        stage_id: "stage-a".to_string(),
        field: AmendmentField::WiringTests,
        patch: AmendmentPatch::Replace {
            index: 5,
            value: make_wiring_test_yaml("nope", "true"),
        },
        reason: None,
        dispute_id: None,
    };
    let err = apply_amendment(&env.plan_path, &env.work_dir, req).unwrap_err();
    let s = format!("{err:#}");
    assert!(s.contains("out of bounds"), "got: {s}");

    // Plan + stage file untouched; no snapshot written.
    assert_eq!(read_plan(&env), PLAN_CONTENT);
    let stage = load_stage("stage-a", &env.work_dir).unwrap();
    assert_eq!(stage.wiring_tests.len(), 1);
    assert_eq!(snapshot_count(&env), 0);
}

// --------------------------------------------------------------------------
// 6f. Invalid WiringTest shape (missing `command`) rejected before any I/O
// --------------------------------------------------------------------------
#[test]
fn invalid_wiring_test_shape_rejected_via_real_type_deserialization() {
    let env = setup_env();
    // WiringTest requires `name` AND `command`. Omitting `command` must be
    // rejected by serde before any file is touched.
    let req = AmendmentRequest {
        stage_id: "stage-a".to_string(),
        field: AmendmentField::WiringTests,
        patch: AmendmentPatch::Replace {
            index: 0,
            value: "name: \"smoke test\"\n".to_string(),
        },
        reason: None,
        dispute_id: None,
    };
    let err = apply_amendment(&env.plan_path, &env.work_dir, req).unwrap_err();
    let s = format!("{err:#}");
    assert!(s.contains("WiringTest"), "got: {s}");

    assert_eq!(read_plan(&env), PLAN_CONTENT);
    assert_eq!(snapshot_count(&env), 0);
}

// --------------------------------------------------------------------------
// 6g. "wiring_tests" round-trips through the audit log's name/parse pair —
// recovery reads the field back off disk (not just writes it), so this
// proves the parse direction, not only the serialize direction.
// --------------------------------------------------------------------------
#[test]
fn wiring_tests_field_name_round_trips_through_recovery() {
    let env = setup_env();
    let req = AmendmentRequest {
        stage_id: "stage-a".to_string(),
        field: AmendmentField::WiringTests,
        patch: AmendmentPatch::Replace {
            index: 0,
            value: make_wiring_test_yaml("smoke test v2", "true"),
        },
        reason: None,
        dispute_id: None,
    };
    apply_amendment(&env.plan_path, &env.work_dir, req).unwrap();

    // Simulate a crash between the plan swap and the stage-file update: roll
    // the stage file back to its pre-amendment content.
    update_stage("stage-a", &env.work_dir, |stage| {
        *stage = make_stage("stage-a");
        Ok(())
    })
    .unwrap();
    let cur = load_stage("stage-a", &env.work_dir).unwrap();
    assert_eq!(cur.wiring_tests[0].name, "smoke test");

    // Recovery must parse the audit row's "wiring_tests" field string back
    // into AmendmentField::WiringTests to know which array to repair.
    let actions = verify_plan_versions_consistency(&env.plan_path, &env.work_dir).unwrap();
    assert!(actions >= 1);
    let recovered = load_stage("stage-a", &env.work_dir).unwrap();
    assert_eq!(recovered.wiring_tests[0].name, "smoke test v2");
}
