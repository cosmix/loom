//! An integration-verify stage's dispute over a completed stage's wiring
//! entry, end to end: an accepted verdict amends THAT stage's wiring, in the
//! plan and its stage file, and re-queues the disputing stage.

use std::path::{Path, PathBuf};

use super::tests::{make_stage, write_request, write_stage, write_verdict};
use super::AdjudicatorRegistry;
use crate::models::dispute::{
    Citation, CriterionField, CriterionTarget, DisputeKind, DisputeVerdict, PlanPatch,
};
use crate::models::stage::{StageStatus, StageType, WiringCheck};
use crate::plan::amendment::{
    apply_amendment, count_amendments_for_stage, AmendmentField, AmendmentPatch, AmendmentRequest,
};
use crate::verify::transitions::load_stage;

/// `up` owns one wiring check the merged tree no longer satisfies; `iv` is
/// the integration-verify stage that re-verifies it.
const PLAN: &str = r#"# Plan

<!-- loom METADATA -->

```yaml
loom:
  version: 1
  stages:
    - id: up
      name: up
      working_dir: "."
      acceptance:
        - "true"
      wiring:
        - source: src/mod.rs
          pattern: "use old_export;"
          description: the old re-export
    - id: iv
      name: iv
      working_dir: "."
      stage_type: integration-verify
      dependencies: [up]
      acceptance:
        - "true"
```

<!-- END loom METADATA -->
"#;

fn setup(root: &Path) -> (PathBuf, PathBuf) {
    let work = root.join(".loom").join("work");
    std::fs::create_dir_all(work.join("stages")).unwrap();
    let plans = root.join("doc").join("plans");
    std::fs::create_dir_all(&plans).unwrap();
    let plan = plans.join("PLAN-owner-wiring.md");
    std::fs::write(&plan, PLAN).unwrap();
    let config = format!(
        "[plan]\nsource_path = \"{}\"\nplan_id = \"x\"\nplan_name = \"x\"\n\
         base_branch = \"main\"\n",
        plan.display()
    );
    std::fs::write(work.join("config.toml"), config).unwrap();
    (work, plan)
}

fn accept_replacing_wiring_0() -> DisputeVerdict {
    DisputeVerdict::Accept {
        plan_patch: PlanPatch {
            inner: serde_json::json!({
                "field": "wiring",
                "patches": [{
                    "op": "replace",
                    "index": 0,
                    "value": "source: src/mod.rs\npattern: \"use new_export;\"\n\
                              description: the new re-export\n",
                }],
                "reason": "the integration fix replaced the export",
            }),
        },
        citations: vec![Citation {
            file: "src/mod.rs".to_string(),
            line: Some(1),
            excerpt: "use new_export;".to_string(),
            claim: "the export moved".to_string(),
        }],
        reasoning: "the old export would be an unused import".to_string(),
    }
}

/// `up` completed and merged with the stale check; `iv` the integration-verify
/// stage in adjudication over dispute 1, which names `up`'s wiring 0.
fn file_owner_dispute(work: &Path) {
    let mut up = make_stage("up");
    up.status = StageStatus::Completed;
    up.merged = true;
    up.completed_commit = Some("abc1234".to_string());
    up.wiring = vec![WiringCheck {
        source: "src/mod.rs".to_string(),
        pattern: "use old_export;".to_string(),
        description: "the old re-export".to_string(),
        literal: false,
    }];
    write_stage(work, &up);
    let mut iv = make_stage("iv");
    iv.stage_type = StageType::IntegrationVerify;
    write_stage(work, &iv);
    let kind = DisputeKind::from_target(CriterionTarget {
        field: CriterionField::Wiring,
        criterion_indices: vec![0],
        owner_stage: Some("up".to_string()),
    });
    write_request(work, "iv", 1, kind);
    write_verdict(work, "iv", 1, accept_replacing_wiring_0(), 1);
}

#[test]
fn an_accepted_owner_wiring_verdict_amends_the_owner_stage() {
    let tmp = tempfile::tempdir().unwrap();
    let (work, plan) = setup(tmp.path());
    file_owner_dispute(&work);

    AdjudicatorRegistry::new()
        .apply_verdict(&work, "iv", 1)
        .unwrap();

    let amended_plan = std::fs::read_to_string(&plan).unwrap();
    assert!(amended_plan.contains("use new_export;"), "{amended_plan}");
    assert!(!amended_plan.contains("use old_export;"), "{amended_plan}");
    let up = load_stage("up", &work).unwrap();
    assert_eq!(up.wiring.len(), 1);
    assert_eq!(up.wiring[0].pattern, "use new_export;");
    assert_eq!(up.status, StageStatus::Completed);
    assert_eq!(
        up.completed_commit.as_deref(),
        Some("abc1234"),
        "a merged owner keeps the commit its merge was verified against"
    );
    let iv = load_stage("iv", &work).unwrap();
    assert!(
        iv.wiring.is_empty(),
        "the filing stage's own list is untouched"
    );
    assert_ne!(iv.status, StageStatus::NeedsAdjudication);
    assert_ne!(iv.status, StageStatus::NeedsHumanReview);
}

/// Dispute ids are numbered per filing stage. The owner's own dispute 1
/// already amended its wiring; the integration-verify stage's dispute 1 is a
/// different verdict and must still apply, not read as that one replayed.
#[test]
fn an_owner_verdict_is_not_mistaken_for_the_owners_own_dispute_of_the_same_id() {
    let tmp = tempfile::tempdir().unwrap();
    let (work, plan) = setup(tmp.path());
    file_owner_dispute(&work);
    let own = AmendmentRequest {
        stage_id: "up".to_string(),
        field: AmendmentField::Wiring,
        patch: AmendmentPatch::Replace {
            index: 0,
            value: "source: src/mod.rs\npattern: \"use mid_export;\"\n\
                    description: the interim re-export\n"
                .to_string(),
        },
        reason: None,
        dispute_id: Some("1.0".to_string()),
    };
    apply_amendment(&plan, &work, own).unwrap();

    AdjudicatorRegistry::new()
        .apply_verdict(&work, "iv", 1)
        .unwrap();

    let amended_plan = std::fs::read_to_string(&plan).unwrap();
    assert!(amended_plan.contains("use new_export;"), "{amended_plan}");
    assert!(!amended_plan.contains("use mid_export;"), "{amended_plan}");
    assert_eq!(count_amendments_for_stage(&work, "up").unwrap(), 2);
    assert_eq!(count_amendments_for_stage(&work, "iv").unwrap(), 0);
}
