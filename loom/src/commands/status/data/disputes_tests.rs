use super::*;
use tempfile::TempDir;

fn record(dir: &Path, id: u32, request: &str, verdict: Option<&str>) {
    let path = dir.join(id.to_string());
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("request.md"), format!("---\n{request}\n---\n")).unwrap();
    if let Some(verdict) = verdict {
        fs::write(path.join("verdict.md"), format!("---\n{verdict}\n---\n")).unwrap();
    }
}

#[test]
fn a_canonical_patches_list_and_a_single_patch_are_both_read() {
    let temp = TempDir::new().unwrap();
    record(
        temp.path(),
        1,
        "kind: criterion\ncriterion_indices: [12, 13]",
        Some(
            "verdict:\n  verdict: accept\n  plan_patch:\n    field: acceptance\n    patches:\n    \
             - {op: replace, index: 12, value: x}\n    - {op: delete, index: 13}",
        ),
    );
    record(
        temp.path(),
        2,
        "kind: criterion\ncriterion_indices: [14]",
        Some(
            "verdict:\n  verdict: accept\n  plan_patch:\n    field: acceptance\n    patch:\n      \
             index: 14\n      op: replace\n      value: y",
        ),
    );

    let lines = read_disputes(temp.path());

    assert_eq!(lines[0].field.as_deref(), Some("acceptance"));
    assert_eq!(
        lines[0].ops,
        [
            DisputeOp {
                op: "replace".into(),
                index: Some(12)
            },
            DisputeOp {
                op: "delete".into(),
                index: Some(13)
            },
        ]
    );
    assert_eq!(
        lines[1].ops,
        [DisputeOp {
            op: "replace".into(),
            index: Some(14)
        }]
    );
    assert_eq!(lines[1].verdict.as_deref(), Some("accept"));
}

#[test]
fn an_empty_patch_leaves_a_ruling_without_edits() {
    let temp = TempDir::new().unwrap();
    record(
        temp.path(),
        1,
        "kind: integrity",
        Some("verdict:\n  verdict: accept\n  plan_patch: {}"),
    );

    let lines = read_disputes(temp.path());

    assert_eq!(lines[0].kind, "integrity");
    assert_eq!(lines[0].verdict.as_deref(), Some("accept"));
    assert!(lines[0].ops.is_empty() && lines[0].field.is_none());
}

#[test]
fn a_malformed_or_missing_record_degrades_to_less_detail() {
    let temp = TempDir::new().unwrap();
    record(
        temp.path(),
        1,
        "kind: [unclosed",
        Some("not frontmatter at all"),
    );
    record(temp.path(), 2, "kind: criterion", None);

    let lines = read_disputes(temp.path());

    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].kind, "dispute");
    assert!(lines[0].verdict.is_none());
    assert_eq!(lines[1].kind, "criterion");
    assert!(lines[1].verdict.is_none());
}

#[test]
fn only_the_newest_disputes_are_read_oldest_first() {
    let temp = TempDir::new().unwrap();
    for id in 1..=8 {
        record(temp.path(), id, "kind: criterion", None);
    }

    let ids: Vec<u32> = read_disputes(temp.path()).iter().map(|d| d.id).collect();

    assert_eq!(ids, [4, 5, 6, 7, 8]);
}

#[test]
fn a_stage_that_is_not_blocked_carries_no_disputes() {
    let temp = TempDir::new().unwrap();
    let work = WorkDir::new(temp.path()).unwrap();
    work.initialize().unwrap();
    let mut stage = Stage::new("server".to_string(), None);
    stage.id = "server".to_string();
    record(
        &work.disputes_dir().join("server"),
        1,
        "kind: criterion",
        None,
    );

    stage.status = StageStatus::Executing;
    assert!(blocked_disputes(&stage, &work).is_empty());
    stage.status = StageStatus::Blocked;
    assert_eq!(blocked_disputes(&stage, &work).len(), 1);
}
