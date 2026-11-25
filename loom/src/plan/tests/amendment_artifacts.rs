//! Integration tests for the `artifacts` amendment field, end to end through
//! `apply_amendment` against a temp `.loom/work/` + plan file layout. Shared
//! fixtures live in `plan::tests::amendment`.

use crate::plan::amendment::{
    apply_amendment, verify_plan_versions_consistency, AmendmentField, AmendmentPatch,
    AmendmentRequest,
};
use crate::verify::transitions::load_stage;

use super::amendment::{
    audit_content, read_plan, setup_env_with_plan, snapshot_count, PLAN_CONTENT,
};

fn plan_with_artifacts() -> String {
    PLAN_CONTENT.replace(
        "      acceptance:\n",
        "      artifacts:\n        - \"docs/old.md\"\n        - \"docs/keep.md\"\n      acceptance:\n",
    )
}

fn artifacts_request(patch: AmendmentPatch) -> AmendmentRequest {
    AmendmentRequest {
        stage_id: "stage-a".to_string(),
        field: AmendmentField::Artifacts,
        patch,
        reason: Some("the pattern names a moved file".to_string()),
        dispute_id: None,
    }
}

#[test]
fn an_artifacts_replace_reaches_the_plan_the_stage_and_the_audit_log() {
    let env = setup_env_with_plan(&plan_with_artifacts());
    let patch = AmendmentPatch::Replace {
        index: 0,
        value: "  docs/new/*.md  ".to_string(),
    };

    apply_amendment(&env.plan_path, &env.work_dir, artifacts_request(patch)).unwrap();

    let plan = read_plan(&env);
    assert!(plan.contains("docs/new/*.md"), "{plan}");
    assert!(!plan.contains("docs/old.md"), "{plan}");
    assert!(plan.contains("docs/keep.md"), "{plan}");
    let stage = load_stage("stage-a", &env.work_dir).unwrap();
    assert_eq!(stage.artifacts, ["docs/new/*.md", "docs/keep.md"]);
    assert!(audit_content(&env).contains("| artifacts |"));
    let repaired = verify_plan_versions_consistency(&env.plan_path, &env.work_dir).unwrap();
    assert_eq!(repaired, 0, "the audited row is consistent with the plan");
}

#[test]
fn an_invalid_artifacts_glob_is_refused_before_anything_is_journalled() {
    let env = setup_env_with_plan(&plan_with_artifacts());
    let before = read_plan(&env);
    let patch = AmendmentPatch::Replace {
        index: 0,
        value: "docs/[x.md".to_string(),
    };

    let error = apply_amendment(&env.plan_path, &env.work_dir, artifacts_request(patch))
        .expect_err("an invalid glob is refused");

    assert!(format!("{error:#}").contains("docs/[x.md"), "{error:#}");
    assert_eq!(read_plan(&env), before, "the plan is untouched");
    assert_eq!(snapshot_count(&env), 0, "no snapshot is written");
    assert!(
        !crate::plan::amendment::plan_versions_dir(&env.work_dir)
            .join("audit.md")
            .exists(),
        "no audit row is written"
    );
}
