use super::*;
use crate::models::stage::Stage;
use crate::verify::transitions::save_stage;
use tempfile::TempDir;

fn fixture() -> (TempDir, PathBuf, PathBuf) {
    let temp = TempDir::new().unwrap();
    let work = temp.path().join(".loom/work");
    fs::create_dir_all(work.join("stages")).unwrap();
    let plan = temp.path().join("plan.md");
    let mut stages = String::new();
    for id in ["a", "b"] {
        stages.push_str(&format!("    - id: {id}\n      name: {id}\n      working_dir: .\n      acceptance: [one, two]\n"));
        let stage = Stage {
            id: id.into(),
            name: id.into(),
            working_dir: Some(".".into()),
            acceptance: vec![
                AcceptanceCriterion::Simple("one".into()),
                AcceptanceCriterion::Simple("two".into()),
            ],
            ..Stage::default()
        };
        save_stage(&stage, &work).unwrap();
    }
    fs::write(&plan, format!("# Plan\n<!-- loom METADATA -->\n```yaml\nloom:\n  version: 1\n  stages:\n{stages}```\n<!-- END loom METADATA -->\n")).unwrap();
    (temp, work, plan)
}

fn request(stage: &str, patch: AmendmentPatch) -> AmendmentRequest {
    AmendmentRequest {
        stage_id: stage.into(),
        field: AmendmentField::Acceptance,
        patch,
        reason: None,
        dispute_id: Some(format!("dispute-{stage}")),
    }
}

fn assert_policy(plan: &Path, work: &Path, stage_id: &str, expected: &[&str]) {
    let content = fs::read_to_string(plan).unwrap();
    let extracted = extract_yaml_metadata_with_ranges(&content).unwrap();
    let metadata = parse_and_validate(&extracted.yaml).unwrap();
    let definition = metadata
        .loom
        .stages
        .iter()
        .find(|s| s.id == stage_id)
        .unwrap();
    let expected: Vec<_> = expected
        .iter()
        .map(|s| AcceptanceCriterion::Simple((*s).into()))
        .collect();
    assert_eq!(definition.acceptance, expected);
    assert_eq!(load_stage(stage_id, work).unwrap().acceptance, expected);
}

fn amendment_files(plan: &Path, work: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut paths = vec![plan.to_path_buf()];
    for dir in [plan_versions_dir(work), work.join("stages")] {
        paths.extend(
            fs::read_dir(dir)
                .unwrap()
                .map(|entry| entry.unwrap().path()),
        );
    }
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let bytes = fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect()
}

#[test]
fn malformed_committed_audit_row_blocks_recovery_and_retry_without_writes() {
    for (cell, bad_value) in [(1, "bad-version"), (3, "bad-field"), (6, "bad-time")] {
        let (_temp, work, plan) = fixture();
        let req = request("a", AmendmentPatch::Delete { index: 0 });
        apply_amendment(&plan, &work, req.clone()).unwrap();
        let dir = plan_versions_dir(&work);
        let audit = dir.join("audit.md");
        let original = fs::read_to_string(&audit).unwrap();
        let row = original.lines().last().unwrap();
        let mut cells: Vec<&str> = row.split('|').collect();
        cells[cell] = bad_value;
        fs::write(&audit, original.replace(row, &cells.join("|"))).unwrap();
        fs::write(dir.join("99.md"), b"orphan snapshot").unwrap();
        fs::write(dir.join("100.md.tmp"), b"temporary snapshot").unwrap();
        let before = amendment_files(&plan, &work);

        let recovery = verify_plan_versions_consistency(&plan, &work).unwrap_err();
        assert!(
            recovery.to_string().contains("audit.md at line 5"),
            "{recovery:#}"
        );
        let retry = apply_amendment(&plan, &work, req.clone()).unwrap_err();
        assert!(
            retry.to_string().contains("audit.md at line 5"),
            "{retry:#}"
        );
        let count = count_amendments_for_stage(&work, "a").unwrap_err();
        assert!(
            count.to_string().contains("audit.md at line 5"),
            "{count:#}"
        );
        assert_eq!(amendment_files(&plan, &work), before);
    }
}

#[test]
fn damaged_audit_framing_is_not_an_empty_history() {
    let (_temp, work, _plan) = fixture();
    let dir = plan_versions_dir(&work);
    fs::create_dir_all(&dir).unwrap();
    assert!(read_audit_rows(&work).unwrap().is_empty());
    let audit = dir.join("audit.md");
    let damaged_row = format!("{AUDIT_HEADER}damaged row\n");
    for body in [
        "",
        "# Plan Amendment Audit\n",
        "# Plan Amendment Audit\n\n| version | stage_id | field | op | index | applied_at | dispute_id | reason |\n",
        "# Plan Amendment Audit\n\n| damaged header |\n",
        AUDIT_HEADER,
        &damaged_row,
    ] {
        fs::write(&audit, body).unwrap();
        assert!(read_audit_rows(&work).is_err(), "accepted {body:?}");
    }
    let valid = format!(
        "{AUDIT_HEADER}| 1 | a | acceptance | delete | 0 | 2026-05-13T07:30:00+00:00 | dispute-a | |\n"
    );
    fs::write(&audit, valid.replace('\n', "\r\n")).unwrap();
    assert_eq!(read_audit_rows(&work).unwrap().len(), 1);
}

#[test]
fn malformed_pending_row_cannot_be_completed_by_retry() {
    let (_temp, work, plan) = fixture();
    let req = request("a", AmendmentPatch::Delete { index: 0 });
    transaction::fail_once(&work, "after-audit");
    assert!(apply_amendment(&plan, &work, req.clone()).is_err());
    let dir = plan_versions_dir(&work);
    assert!(!dir.join("1.applied").exists());
    let audit = dir.join("audit.md");
    let content = fs::read_to_string(&audit).unwrap();
    fs::write(
        &audit,
        content.replace("| acceptance |", "| invalid-field |"),
    )
    .unwrap();
    let before = amendment_files(&plan, &work);

    let error = apply_amendment(&plan, &work, req).unwrap_err();
    assert!(
        error.to_string().contains("audit.md at line 5"),
        "{error:#}"
    );
    assert_eq!(amendment_files(&plan, &work), before);
    assert!(!dir.join("1.applied").exists());
}

fn interrupted_retry(phase: &'static str, patch: AmendmentPatch, expected: &[&str]) {
    let (_temp, work, plan) = fixture();
    let req = request("a", patch);
    transaction::fail_once(&work, phase);
    assert!(apply_amendment(&plan, &work, req.clone()).is_err());
    assert!(!plan_versions_dir(&work).join("1.applied").exists());
    assert_eq!(load_stage("a", &work).unwrap().acceptance.len(), 2);
    let result = apply_amendment(&plan, &work, req.clone()).unwrap();
    assert_eq!(result.version, 1);
    assert_policy(&plan, &work, "a", expected);
    assert!(plan_versions_dir(&work).join("1.applied").exists());
    apply_amendment(&plan, &work, req).unwrap();
    assert_policy(&plan, &work, "a", expected);
    assert_eq!(read_audit_rows(&work).unwrap().len(), 1);
}

#[test]
fn retry_after_audit_finishes_insert_snapshot_once() {
    interrupted_retry(
        "after-audit",
        AmendmentPatch::Insert {
            index: 0,
            value: "inserted".into(),
        },
        &["inserted", "one", "two"],
    );
}

#[test]
fn retry_after_plan_finishes_delete_snapshot_once() {
    interrupted_retry("after-plan", AmendmentPatch::Delete { index: 0 }, &["two"]);
}

#[test]
fn intervening_stage_amendment_finishes_pending_snapshot_without_losing_either() {
    let (_temp, work, plan) = fixture();
    let a = request(
        "a",
        AmendmentPatch::Insert {
            index: 0,
            value: "inserted".into(),
        },
    );
    transaction::fail_once(&work, "after-audit");
    assert!(apply_amendment(&plan, &work, a.clone()).is_err());
    let b = request("b", AmendmentPatch::Delete { index: 0 });
    assert_eq!(apply_amendment(&plan, &work, b).unwrap().version, 2);
    assert_eq!(apply_amendment(&plan, &work, a).unwrap().version, 1);
    assert_policy(&plan, &work, "a", &["inserted", "one", "two"]);
    assert_policy(&plan, &work, "b", &["two"]);
    assert_eq!(read_audit_rows(&work).unwrap().len(), 2);
}

#[test]
fn failed_stage_write_cannot_create_receipt_or_return_idempotent_success() {
    let (_temp, work, plan) = fixture();
    let a = request("a", AmendmentPatch::Delete { index: 0 });
    transaction::fail_once(&work, "after-audit");
    assert!(apply_amendment(&plan, &work, a.clone()).is_err());
    let stages = work.join("stages");
    fs::rename(&stages, work.join("saved-stages")).unwrap();
    assert!(apply_amendment(&plan, &work, a.clone()).is_err());
    assert!(!plan_versions_dir(&work).join("1.applied").exists());
    fs::remove_dir_all(&stages).ok();
    fs::rename(work.join("saved-stages"), &stages).unwrap();
    apply_amendment(&plan, &work, a).unwrap();
    assert_policy(&plan, &work, "a", &["two"]);
}

#[path = "retry_recovery_tests.rs"]
mod retry_recovery_tests;
