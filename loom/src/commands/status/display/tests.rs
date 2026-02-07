use crate::models::stage::StageStatus;
use crate::verify::transitions::parse_stage_from_markdown;

#[test]
fn test_parse_stage_with_retry_info() {
    use crate::models::failure::FailureType;

    let content = r#"---
id: stage-test-1
name: Test Stage
status: blocked
dependencies: []
acceptance: []
setup: []
files: []
child_stages: []
retry_count: 2
max_retries: 3
created_at: 2025-01-10T12:00:00Z
updated_at: 2025-01-10T12:00:00Z
failure_info:
  failure_type: session-crash
  detected_at: 2025-01-10T12:00:00Z
  evidence:
    - "Session crashed unexpectedly"
---

# Stage: Test Stage
"#;

    let stage = parse_stage_from_markdown(content).expect("Should parse stage from markdown");

    assert_eq!(stage.id, "stage-test-1");
    assert_eq!(stage.name, "Test Stage");
    assert_eq!(stage.status, StageStatus::Blocked);
    assert_eq!(stage.retry_count, 2);
    assert_eq!(stage.max_retries, Some(3));
    assert!(stage.failure_info.is_some());

    if let Some(failure_info) = stage.failure_info {
        assert_eq!(failure_info.failure_type, FailureType::SessionCrash);
        assert_eq!(failure_info.evidence.len(), 1);
    }
}

#[test]
fn test_parse_stage_skipped() {
    let content = r#"---
id: stage-test-2
name: Skipped Stage
status: skipped
dependencies: []
acceptance: []
setup: []
files: []
child_stages: []
created_at: 2025-01-10T12:00:00Z
updated_at: 2025-01-10T12:00:00Z
---

# Stage: Skipped Stage
"#;

    let stage = parse_stage_from_markdown(content).expect("Should parse stage from markdown");

    assert_eq!(stage.id, "stage-test-2");
    assert_eq!(stage.name, "Skipped Stage");
    assert_eq!(stage.status, StageStatus::Skipped);
}

#[test]
fn test_parse_stage_needs_human_review() {
    let content = r#"---
id: stage-test-3
name: Review Required Stage
status: needs-human-review
dependencies: []
acceptance: []
setup: []
files: []
child_stages: []
created_at: 2025-01-10T12:00:00Z
updated_at: 2025-01-10T12:00:00Z
review_reason: "Acceptance criteria appear to test implementation details rather than behavior"
---

# Stage: Review Required Stage
"#;

    let stage = parse_stage_from_markdown(content).expect("Should parse stage from markdown");

    assert_eq!(stage.id, "stage-test-3");
    assert_eq!(stage.name, "Review Required Stage");
    assert_eq!(stage.status, StageStatus::NeedsHumanReview);
    assert!(stage.review_reason.is_some());
    assert_eq!(
        stage.review_reason.unwrap(),
        "Acceptance criteria appear to test implementation details rather than behavior"
    );
}
