//! Integration tests for runtime plan amendment.
//!
//! These tests exercise `apply_amendment` and `verify_plan_versions_consistency`
//! end-to-end against a temp `.work/` + plan file layout.

use std::fs;
use std::path::PathBuf;

use tempfile::TempDir;

use crate::models::stage::Stage;
use crate::plan::amendment::{
    apply_amendment, count_amendments_for_stage, plan_versions_dir,
    verify_plan_versions_consistency, AmendmentField, AmendmentPatch, AmendmentRequest,
};
use crate::plan::schema::AcceptanceCriterion;
use crate::verify::transitions::{load_stage, save_stage};

const PLAN_CONTENT: &str = "# PLAN: Amendment Test\n\n\
Human-readable section with **prose**, lists:\n\
- bullet 1\n\
- bullet 2\n\n\
And a code example:\n\n\
```rust\n\
fn placeholder() {}\n\
```\n\n\
<!-- loom METADATA -->\n\n\
```yaml\n\
loom:\n  version: 1\n  adjudication:\n    max_amendments_per_stage: 3\n  stages:\n    - id: stage-a\n      name: \"Alpha\"\n      working_dir: \".\"\n      dependencies: []\n      acceptance:\n        - \"cargo test\"\n        - \"cargo clippy\"\n      wiring:\n        - source: \"src/lib.rs\"\n          pattern: \"pub fn foo\"\n          description: \"foo is exported\"\n```\n\n\
<!-- END loom METADATA -->\n\n\
Trailing prose with **more** content.\n";

struct TestEnv {
    _tmp: TempDir,
    work_dir: PathBuf,
    plan_path: PathBuf,
}

fn setup_env() -> TestEnv {
    setup_env_with_plan(PLAN_CONTENT)
}

fn setup_env_with_plan(plan_text: &str) -> TestEnv {
    let tmp = TempDir::new().unwrap();
    let project_root = tmp.path().to_path_buf();
    let work_dir = project_root.join(".work");
    fs::create_dir_all(work_dir.join("stages")).unwrap();
    fs::create_dir_all(project_root.join("doc").join("plans")).unwrap();

    // Plan file under project_root/doc/plans/PLAN-amendment-test.md.
    let plan_path = project_root.join("doc").join("plans").join("PLAN-amendment-test.md");
    fs::write(&plan_path, plan_text).unwrap();

    // .work/config.toml pointing at the plan file (absolute path).
    let cfg = format!(
        "[plan]\nsource_path = \"{}\"\nplan_id = \"x\"\nplan_name = \"X\"\nbase_branch = \"main\"\n",
        plan_path.display(),
    );
    fs::write(work_dir.join("config.toml"), cfg).unwrap();

    // Write the matching stage file.
    let stage = make_stage("stage-a");
    save_stage(&stage, &work_dir).unwrap();

    TestEnv {
        _tmp: tmp,
        work_dir,
        plan_path,
    }
}

fn make_stage(id: &str) -> Stage {
    Stage {
        id: id.to_string(),
        name: format!("Stage {id}"),
        working_dir: Some(".".to_string()),
        acceptance: vec![
            AcceptanceCriterion::Simple("cargo test".to_string()),
            AcceptanceCriterion::Simple("cargo clippy".to_string()),
        ],
        wiring: vec![crate::models::stage::WiringCheck {
            source: "src/lib.rs".to_string(),
            pattern: "pub fn foo".to_string(),
            description: "foo is exported".to_string(),
        }],
        ..Stage::default()
    }
}

fn read_plan(env: &TestEnv) -> String {
    fs::read_to_string(&env.plan_path).unwrap()
}

fn audit_content(env: &TestEnv) -> String {
    let p = plan_versions_dir(&env.work_dir).join("audit.md");
    fs::read_to_string(p).unwrap()
}

fn snapshot_count(env: &TestEnv) -> usize {
    let dir = plan_versions_dir(&env.work_dir);
    if !dir.exists() {
        return 0;
    }
    fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| {
            let name = e.ok()?.file_name();
            let s = name.to_str()?.to_string();
            if s.ends_with(".md") && !s.ends_with(".md.tmp") && s != "audit.md" {
                Some(())
            } else {
                None
            }
        })
        .count()
}

fn make_acceptance_yaml(cmd: &str) -> String {
    // YAML body for an AcceptanceCriterion::Simple — plain string.
    format!("\"{cmd}\"")
}

fn make_wiring_yaml(source: &str, pattern: &str, description: &str) -> String {
    format!(
        "source: \"{source}\"\npattern: \"{pattern}\"\ndescription: \"{description}\"\n"
    )
}

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
// 2. Insert acceptance
// --------------------------------------------------------------------------
#[test]
fn insert_acceptance_at_end_appends() {
    let env = setup_env();
    let req = AmendmentRequest {
        stage_id: "stage-a".to_string(),
        field: AmendmentField::Acceptance,
        patch: AmendmentPatch::Insert {
            index: 2,
            value: make_acceptance_yaml("cargo fmt --check"),
        },
        reason: None,
        dispute_id: None,
    };
    apply_amendment(&env.plan_path, &env.work_dir, req).unwrap();
    let stage = load_stage("stage-a", &env.work_dir).unwrap();
    assert_eq!(stage.acceptance.len(), 3);
    assert_eq!(stage.acceptance[2].command(), "cargo fmt --check");
}

// --------------------------------------------------------------------------
// 3. Delete acceptance
// --------------------------------------------------------------------------
#[test]
fn delete_acceptance_removes_entry() {
    let env = setup_env();
    let req = AmendmentRequest {
        stage_id: "stage-a".to_string(),
        field: AmendmentField::Acceptance,
        patch: AmendmentPatch::Delete { index: 1 },
        reason: None,
        dispute_id: None,
    };
    apply_amendment(&env.plan_path, &env.work_dir, req).unwrap();
    let stage = load_stage("stage-a", &env.work_dir).unwrap();
    assert_eq!(stage.acceptance.len(), 1);
    assert_eq!(stage.acceptance[0].command(), "cargo test");
}

// --------------------------------------------------------------------------
// 4. Replace wiring
// --------------------------------------------------------------------------
#[test]
fn replace_wiring_succeeds() {
    let env = setup_env();
    let req = AmendmentRequest {
        stage_id: "stage-a".to_string(),
        field: AmendmentField::Wiring,
        patch: AmendmentPatch::Replace {
            index: 0,
            value: make_wiring_yaml("src/foo.rs", "fn bar", "bar exists"),
        },
        reason: None,
        dispute_id: None,
    };
    apply_amendment(&env.plan_path, &env.work_dir, req).unwrap();
    let stage = load_stage("stage-a", &env.work_dir).unwrap();
    assert_eq!(stage.wiring.len(), 1);
    assert_eq!(stage.wiring[0].source, "src/foo.rs");
    assert_eq!(stage.wiring[0].pattern, "fn bar");
}

// --------------------------------------------------------------------------
// 5. Insert wiring
// --------------------------------------------------------------------------
#[test]
fn insert_wiring_extends_list() {
    let env = setup_env();
    let req = AmendmentRequest {
        stage_id: "stage-a".to_string(),
        field: AmendmentField::Wiring,
        patch: AmendmentPatch::Insert {
            index: 1,
            value: make_wiring_yaml("src/lib.rs", "pub fn bar", "bar exported"),
        },
        reason: None,
        dispute_id: None,
    };
    apply_amendment(&env.plan_path, &env.work_dir, req).unwrap();
    let stage = load_stage("stage-a", &env.work_dir).unwrap();
    assert_eq!(stage.wiring.len(), 2);
    assert_eq!(stage.wiring[1].pattern, "pub fn bar");
}

// --------------------------------------------------------------------------
// 6. Delete wiring
// --------------------------------------------------------------------------
#[test]
fn delete_wiring_removes_entry() {
    let env = setup_env();
    let req = AmendmentRequest {
        stage_id: "stage-a".to_string(),
        field: AmendmentField::Wiring,
        patch: AmendmentPatch::Delete { index: 0 },
        reason: None,
        dispute_id: None,
    };
    apply_amendment(&env.plan_path, &env.work_dir, req).unwrap();
    let stage = load_stage("stage-a", &env.work_dir).unwrap();
    assert!(stage.wiring.is_empty());
}

// --------------------------------------------------------------------------
// 7. Enum blocks bad amendment — out-of-bounds delete on wiring
// --------------------------------------------------------------------------
#[test]
fn enum_blocks_bad_amendment_out_of_bounds_delete() {
    let env = setup_env();
    let req = AmendmentRequest {
        stage_id: "stage-a".to_string(),
        field: AmendmentField::Wiring,
        patch: AmendmentPatch::Delete { index: 99 },
        reason: None,
        dispute_id: None,
    };
    let err = apply_amendment(&env.plan_path, &env.work_dir, req).unwrap_err();
    let s = format!("{err:#}");
    assert!(s.contains("out of bounds"), "got: {s}");

    // Plan + stage file untouched.
    assert_eq!(read_plan(&env), PLAN_CONTENT);
    let stage = load_stage("stage-a", &env.work_dir).unwrap();
    assert_eq!(stage.wiring.len(), 1);
}

// --------------------------------------------------------------------------
// 8. Absolute cap exceeded
// --------------------------------------------------------------------------
#[test]
fn amendment_cap_blocks_after_three() {
    let env = setup_env();
    for i in 0..3 {
        let req = AmendmentRequest {
            stage_id: "stage-a".to_string(),
            field: AmendmentField::Acceptance,
            patch: AmendmentPatch::Replace {
                index: 0,
                value: make_acceptance_yaml(&format!("cargo test --variant-{i}")),
            },
            reason: None,
            dispute_id: None,
        };
        apply_amendment(&env.plan_path, &env.work_dir, req).unwrap();
    }
    assert_eq!(count_amendments_for_stage(&env.work_dir, "stage-a").unwrap(), 3);

    // Fourth must fail.
    let req = AmendmentRequest {
        stage_id: "stage-a".to_string(),
        field: AmendmentField::Acceptance,
        patch: AmendmentPatch::Replace {
            index: 0,
            value: make_acceptance_yaml("cargo test --rejected"),
        },
        reason: None,
        dispute_id: None,
    };
    let err = apply_amendment(&env.plan_path, &env.work_dir, req).unwrap_err();
    let s = format!("{err:#}");
    assert!(s.contains("amendment cap"), "got: {s}");
}

// --------------------------------------------------------------------------
// 9. Invalid value shape rejected via real type deserialization
// --------------------------------------------------------------------------
#[test]
fn invalid_value_shape_rejected_via_real_type_deserialization() {
    let env = setup_env();
    // A WiringCheck requires `source`, `pattern`, `description`. Sending a
    // bare integer must be rejected by serde — NOT silently accepted by a
    // hand-rolled "simplified" shape.
    let req = AmendmentRequest {
        stage_id: "stage-a".to_string(),
        field: AmendmentField::Wiring,
        patch: AmendmentPatch::Replace {
            index: 0,
            value: "42".to_string(),
        },
        reason: None,
        dispute_id: None,
    };
    let err = apply_amendment(&env.plan_path, &env.work_dir, req).unwrap_err();
    let s = format!("{err:#}");
    assert!(s.contains("WiringCheck"), "got: {s}");

    // Plan and stage file untouched.
    assert_eq!(read_plan(&env), PLAN_CONTENT);
}

// --------------------------------------------------------------------------
// 10. Human-readable preservation byte-for-byte
// --------------------------------------------------------------------------
#[test]
fn human_readable_preserved_byte_for_byte() {
    let env = setup_env();
    let req = AmendmentRequest {
        stage_id: "stage-a".to_string(),
        field: AmendmentField::Acceptance,
        patch: AmendmentPatch::Replace {
            index: 0,
            value: make_acceptance_yaml("cargo test --release"),
        },
        reason: None,
        dispute_id: None,
    };
    apply_amendment(&env.plan_path, &env.work_dir, req).unwrap();
    let new_plan = read_plan(&env);

    // Prefix (everything before the metadata block) must match byte-for-byte.
    let orig_start = PLAN_CONTENT.find("<!-- loom METADATA").unwrap();
    let new_start = new_plan.find("<!-- loom METADATA").unwrap();
    assert_eq!(&PLAN_CONTENT[..orig_start], &new_plan[..new_start]);

    // Suffix (everything from the END marker onward) must match byte-for-byte.
    let orig_end_marker = PLAN_CONTENT.find("<!-- END loom METADATA").unwrap();
    let new_end_marker = new_plan.find("<!-- END loom METADATA").unwrap();
    assert_eq!(&PLAN_CONTENT[orig_end_marker..], &new_plan[new_end_marker..]);
}

// --------------------------------------------------------------------------
// 11. Audit log content
// --------------------------------------------------------------------------
#[test]
fn audit_log_records_amendment() {
    let env = setup_env();
    let req = AmendmentRequest {
        stage_id: "stage-a".to_string(),
        field: AmendmentField::Acceptance,
        patch: AmendmentPatch::Replace {
            index: 1,
            value: make_acceptance_yaml("cargo clippy -- -D warnings"),
        },
        reason: Some("tighten clippy".to_string()),
        dispute_id: Some("d-7".to_string()),
    };
    apply_amendment(&env.plan_path, &env.work_dir, req).unwrap();
    let log = audit_content(&env);
    assert!(log.contains("| 1 |"));
    assert!(log.contains("stage-a"));
    assert!(log.contains("acceptance"));
    assert!(log.contains("replace"));
    assert!(log.contains("d-7"));
    assert!(log.contains("tighten clippy"));
}

// --------------------------------------------------------------------------
// 12. Recovery — snapshot without audit
// --------------------------------------------------------------------------
#[test]
fn recovery_removes_snapshot_without_audit() {
    let env = setup_env();
    // Apply one valid amendment so the directory exists.
    let req = AmendmentRequest {
        stage_id: "stage-a".to_string(),
        field: AmendmentField::Acceptance,
        patch: AmendmentPatch::Replace {
            index: 0,
            value: make_acceptance_yaml("cargo test --release"),
        },
        reason: None,
        dispute_id: None,
    };
    apply_amendment(&env.plan_path, &env.work_dir, req).unwrap();

    // Plant an orphaned snapshot (no audit row).
    let dir = plan_versions_dir(&env.work_dir);
    let orphan = dir.join("99.md");
    fs::write(&orphan, "orphan snapshot").unwrap();
    assert!(orphan.exists());

    // Recovery should drop the orphan.
    let actions = verify_plan_versions_consistency(&env.plan_path, &env.work_dir).unwrap();
    assert!(actions >= 1);
    assert!(!orphan.exists(), "orphan snapshot must be removed");
}

// --------------------------------------------------------------------------
// 13. Recovery — audit without plan swap
// --------------------------------------------------------------------------
#[test]
fn recovery_replays_plan_when_audit_ahead() {
    let env = setup_env();
    // Apply an amendment normally.
    let req = AmendmentRequest {
        stage_id: "stage-a".to_string(),
        field: AmendmentField::Acceptance,
        patch: AmendmentPatch::Replace {
            index: 0,
            value: make_acceptance_yaml("cargo test --release"),
        },
        reason: None,
        dispute_id: None,
    };
    apply_amendment(&env.plan_path, &env.work_dir, req).unwrap();

    // Simulate a crash AFTER the audit row was appended but BEFORE the plan
    // was swapped: roll the plan file back to its original content.
    fs::write(&env.plan_path, PLAN_CONTENT).unwrap();
    // Sanity: the plan file no longer matches the snapshot.
    let dir = plan_versions_dir(&env.work_dir);
    let snap = fs::read_to_string(dir.join("1.md")).unwrap();
    assert_ne!(read_plan(&env), snap);

    // Recovery should rewrite the plan from the snapshot.
    let actions = verify_plan_versions_consistency(&env.plan_path, &env.work_dir).unwrap();
    assert!(actions >= 1);
    assert_eq!(read_plan(&env), snap);
}

// --------------------------------------------------------------------------
// 14. Recovery — plan swapped, stage file stale
// --------------------------------------------------------------------------
#[test]
fn recovery_replays_stage_when_only_stage_file_is_stale() {
    let env = setup_env();
    let req = AmendmentRequest {
        stage_id: "stage-a".to_string(),
        field: AmendmentField::Acceptance,
        patch: AmendmentPatch::Replace {
            index: 0,
            value: make_acceptance_yaml("cargo test --release"),
        },
        reason: None,
        dispute_id: None,
    };
    apply_amendment(&env.plan_path, &env.work_dir, req).unwrap();

    // Roll the stage file back to its ORIGINAL content (simulates a crash
    // between safe_replace_outside_workdir and save_stage at step 10).
    save_stage(&make_stage("stage-a"), &env.work_dir).unwrap();
    let cur = load_stage("stage-a", &env.work_dir).unwrap();
    assert_eq!(cur.acceptance[0].command(), "cargo test");

    // Recovery should re-save the stage from the snapshot.
    let actions = verify_plan_versions_consistency(&env.plan_path, &env.work_dir).unwrap();
    assert!(actions >= 1);
    let recovered = load_stage("stage-a", &env.work_dir).unwrap();
    assert_eq!(recovered.acceptance[0].command(), "cargo test --release");
}

// --------------------------------------------------------------------------
// 15. CRLF preservation
// --------------------------------------------------------------------------
#[test]
fn crlf_line_endings_preserved_in_prose() {
    // Convert PLAN_CONTENT to CRLF except inside the YAML body (so YAML
    // parses cleanly; we only check that the surrounding prose retains its
    // CRLF byte-for-byte through the splice).
    let crlf_plan = PLAN_CONTENT.replace('\n', "\r\n");
    let env = setup_env_with_plan(&crlf_plan);

    let req = AmendmentRequest {
        stage_id: "stage-a".to_string(),
        field: AmendmentField::Acceptance,
        patch: AmendmentPatch::Replace {
            index: 0,
            value: make_acceptance_yaml("cargo test --release"),
        },
        reason: None,
        dispute_id: None,
    };
    apply_amendment(&env.plan_path, &env.work_dir, req).unwrap();
    let new_plan = read_plan(&env);

    // Prefix (before metadata block) is preserved byte-for-byte INCLUDING
    // \r\n endings.
    let orig_start = crlf_plan.find("<!-- loom METADATA").unwrap();
    let new_start = new_plan.find("<!-- loom METADATA").unwrap();
    assert_eq!(&crlf_plan[..orig_start], &new_plan[..new_start]);
    assert!(crlf_plan[..orig_start].contains("\r\n"));

    // Suffix (from END marker onward) is preserved byte-for-byte.
    let orig_end = crlf_plan.find("<!-- END loom METADATA").unwrap();
    let new_end = new_plan.find("<!-- END loom METADATA").unwrap();
    assert_eq!(&crlf_plan[orig_end..], &new_plan[new_end..]);
}

// --------------------------------------------------------------------------
// Path-confinement sanity check for safe_replace_outside_workdir
// --------------------------------------------------------------------------
#[test]
fn replace_refuses_plan_outside_project_root() {
    let env = setup_env();
    // Place a sibling plan file OUTSIDE the project_root. apply_amendment
    // derives project_root from work_dir.parent(), so anything outside
    // that tree must be rejected.
    let outside = tempfile::tempdir().unwrap();
    let foreign_plan = outside.path().join("PLAN.md");
    fs::write(&foreign_plan, PLAN_CONTENT).unwrap();

    let req = AmendmentRequest {
        stage_id: "stage-a".to_string(),
        field: AmendmentField::Acceptance,
        patch: AmendmentPatch::Replace {
            index: 0,
            value: make_acceptance_yaml("cargo test --release"),
        },
        reason: None,
        dispute_id: None,
    };
    let err = apply_amendment(&foreign_plan, &env.work_dir, req).unwrap_err();
    let s = format!("{err:#}");
    assert!(
        s.contains("not under project_root") || s.contains("refusing replace"),
        "got: {s}"
    );
}
