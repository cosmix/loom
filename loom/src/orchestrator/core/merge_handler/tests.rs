#[test]
fn test_merge_blocked_to_completed_transition_is_valid() {
    use crate::models::stage::StageStatus;

    let status = StageStatus::MergeBlocked;
    assert!(
        status.can_transition_to(&StageStatus::Completed),
        "MergeBlocked -> Completed should be a valid transition"
    );

    let result = status.try_transition(StageStatus::Completed);
    assert!(
        result.is_ok(),
        "MergeBlocked -> Completed transition should succeed"
    );
}

#[test]
fn test_merge_conflict_to_completed_transition_is_valid() {
    use crate::models::stage::StageStatus;

    let status = StageStatus::MergeConflict;
    assert!(
        status.can_transition_to(&StageStatus::Completed),
        "MergeConflict -> Completed should be a valid transition"
    );
}

#[test]
fn test_merge_states_to_human_review_require_forced_assignment() {
    // O-3 escalation routes an exhausted-budget merge stage to
    // NeedsHumanReview. That edge is intentionally NOT in the transition
    // table (MergeConflict/MergeBlocked only legally go to Completed/Blocked
    // /Queued/Executing), so `escalate_merge_resolver_exhausted` MUST use
    // `force_status_with_reason`. If a future change makes this edge legal,
    // update the escalation path to prefer a `try_*` transition.
    use crate::models::stage::StageStatus;

    assert!(
        !StageStatus::MergeConflict.can_transition_to(&StageStatus::NeedsHumanReview),
        "MergeConflict -> NeedsHumanReview is expected to be illegal (escalation forces it)"
    );
    assert!(
        !StageStatus::MergeBlocked.can_transition_to(&StageStatus::NeedsHumanReview),
        "MergeBlocked -> NeedsHumanReview is expected to be illegal (escalation forces it)"
    );
}

#[test]
fn test_plan_auto_merge_extraction_true() {
    let plan_content = r#"# Test Plan

<!-- loom METADATA -->

```yaml
loom:
  version: 1
  auto_merge: true
  stages:
    - id: test-stage
      name: "Test"
      stage_type: knowledge
      working_dir: "."
      dependencies: []
      acceptance: []
```

<!-- END loom METADATA -->
"#;
    let yaml_content = crate::plan::parser::extract_yaml_metadata(plan_content).unwrap();
    let metadata = crate::plan::parser::parse_and_validate(&yaml_content).unwrap();
    assert_eq!(metadata.loom.auto_merge, Some(true));
}

#[test]
fn test_plan_auto_merge_extraction_false() {
    let plan_content = r#"# Test Plan

<!-- loom METADATA -->

```yaml
loom:
  version: 1
  auto_merge: false
  stages:
    - id: test-stage
      name: "Test"
      stage_type: knowledge
      working_dir: "."
      dependencies: []
      acceptance: []
```

<!-- END loom METADATA -->
"#;
    let yaml_content = crate::plan::parser::extract_yaml_metadata(plan_content).unwrap();
    let metadata = crate::plan::parser::parse_and_validate(&yaml_content).unwrap();
    assert_eq!(metadata.loom.auto_merge, Some(false));
}

#[test]
fn test_plan_auto_merge_default_none() {
    let plan_content = r#"# Test Plan

<!-- loom METADATA -->

```yaml
loom:
  version: 1
  stages:
    - id: test-stage
      name: "Test"
      stage_type: knowledge
      working_dir: "."
      dependencies: []
      acceptance: []
```

<!-- END loom METADATA -->
"#;
    let yaml_content = crate::plan::parser::extract_yaml_metadata(plan_content).unwrap();
    let metadata = crate::plan::parser::parse_and_validate(&yaml_content).unwrap();
    assert_eq!(metadata.loom.auto_merge, None);
}
