//! The briefing for an integration-verify stage's dispute over a completed
//! stage's wiring entry: the owner's list, observed from the owner's
//! directory, judged without `loom check`, and recorded by the filing stage.

use super::*;
use crate::models::dispute::CriterionTarget;
use crate::models::stage::{StageStatus, StageType, WiringCheck};
use chrono::Utc;

const PLAN: &str = r#"# Plan

<!-- loom METADATA -->

```yaml
loom:
  version: 2
  stages:
    - id: up
      name: up
      working_dir: "loom"
      wiring:
        - source: src/mod.rs
          pattern: "use kept;"
          description: up-plan-marker
        - source: src/mod.rs
          pattern: "use old_export;"
          description: the old re-export
    - id: iv
      name: iv
      working_dir: "."
      stage_type: integration-verify
      dependencies: [up]
```

<!-- END loom METADATA -->
"#;

fn check(pattern: &str, description: &str) -> WiringCheck {
    WiringCheck {
        source: "src/mod.rs".to_string(),
        pattern: pattern.to_string(),
        description: description.to_string(),
        literal: false,
    }
}

fn owner_request() -> DisputeRequest {
    DisputeRequest {
        id: 2,
        stage_id: "iv".to_string(),
        kind: DisputeKind::from_target(CriterionTarget {
            field: CriterionField::Wiring,
            criterion_indices: vec![1],
            owner_stage: Some("up".to_string()),
        }),
        reason: "the integration fix replaced the export".to_string(),
        evidence_commit: None,
        failure_output: None,
        fix_attempts_at_dispute: 0,
        created_at: Utc::now(),
    }
}

/// The briefing for `iv`'s dispute 2 over `up`'s wiring 1, built in `root`.
fn owner_briefing(root: &std::path::Path) -> Prompt {
    let plan = root.join("PLAN.md");
    std::fs::write(&plan, PLAN).unwrap();
    let work = root.join(".loom").join("work");
    std::fs::create_dir_all(work.join("stages")).unwrap();
    let up = Stage {
        id: "up".to_string(),
        name: "up".to_string(),
        status: StageStatus::Completed,
        plan_version: 2,
        working_dir: Some("loom".to_string()),
        wiring: vec![
            check("use kept;", "kept"),
            check("use old_export;", "the old re-export"),
        ],
        ..Stage::default()
    };
    crate::verify::transitions::save_stage(&up, &work).unwrap();
    let iv = Stage {
        id: "iv".to_string(),
        name: "iv".to_string(),
        stage_type: StageType::IntegrationVerify,
        plan_version: 2,
        wiring: vec![check("iv_own_marker", "the filing stage's own check")],
        ..Stage::default()
    };
    build(&plan, &iv, &owner_request(), &work)
}

#[test]
fn an_owner_wiring_dispute_briefs_the_owners_list_without_loom_check() {
    let tmp = tempfile::tempdir().unwrap();
    let prompt = owner_briefing(tmp.path());
    let instructions = &prompt.instructions;

    assert!(
        instructions.contains("entries 1 of\nstage 'up''s `wiring` list"),
        "{instructions}"
    );
    assert!(instructions.contains("does NOT run them"), "{instructions}");
    assert!(
        !instructions.contains("First run `loom check"),
        "{instructions}"
    );
    assert!(
        !instructions.contains("every other wiring check `loom check` reports"),
        "{instructions}"
    );
    assert!(instructions.contains("`up: [wiring n]`"), "{instructions}");
    assert!(
        instructions.contains("- pattern: `use old_export;`"),
        "{instructions}"
    );
    let owner_dir = tmp.path().join("loom");
    assert!(
        instructions.contains(&format!("cd {}\n", owner_dir.display())),
        "observed from the owner's working_dir:\n{instructions}"
    );
    assert!(
        instructions.contains("may amend only\nstage 'up''s `wiring` list"),
        "{instructions}"
    );
    assert!(
        instructions.contains("loom stage adjudicate --stage iv --dispute 2"),
        "the filing stage records the verdict:\n{instructions}"
    );
    assert!(!instructions.contains("iv_own_marker"), "{instructions}");
}

#[test]
fn an_owner_wiring_dispute_quotes_the_owners_list_and_plan_block() {
    let tmp = tempfile::tempdir().unwrap();
    let evidence = owner_briefing(tmp.path()).evidence;

    assert!(evidence.contains("Stage: iv\n"), "{evidence}");
    assert!(evidence.contains("Owner stage: up\n"), "{evidence}");
    assert!(evidence.contains("working_dir: `loom`"), "{evidence}");
    assert!(
        evidence.contains("## Stage 'up' wiring checks (all)"),
        "{evidence}"
    );
    assert!(
        evidence.contains("→ [1] `use old_export;` in `src/mod.rs`"),
        "{evidence}"
    );
    assert!(evidence.contains("  [0] `use kept;`"), "{evidence}");
    assert!(!evidence.contains("iv_own_marker"), "{evidence}");
    assert!(
        evidence.contains("up-plan-marker"),
        "the plan excerpt is the owner's block:\n{evidence}"
    );
}

/// An owner whose stage file cannot be read leaves no entry to observe: every
/// disputed index reads as gone rather than as the filing stage's own entry.
#[test]
fn an_unreadable_owner_reads_every_index_as_gone() {
    let tmp = tempfile::tempdir().unwrap();
    let plan = tmp.path().join("PLAN.md");
    std::fs::write(&plan, PLAN).unwrap();
    let work = tmp.path().join(".loom").join("work");
    std::fs::create_dir_all(work.join("stages")).unwrap();
    let iv = Stage {
        id: "iv".to_string(),
        wiring: vec![check("iv_own_0", "own"), check("iv_own_marker", "own")],
        ..Stage::default()
    };
    let prompt = build(&plan, &iv, &owner_request(), &work);
    assert!(
        prompt
            .instructions
            .contains("no longer has a wiring check at index 1"),
        "{}",
        prompt.instructions
    );
    assert!(
        !prompt.render().contains("iv_own_marker"),
        "{}",
        prompt.render()
    );
}
