//! The review gate and the store it reads (DESIGN D12).

use super::*;
use crate::verify::contracts::test_support::{contract_worktree, CONTRACT_FILE};
use crate::verify::review::fingerprint::ChangeFingerprint;
use crate::verify::review::report::Finding;
use crate::verify::review::store::{
    finding_id, next_round, open_findings, write_round, RecordedFinding, RECORD_VERSION,
};
use std::collections::BTreeMap;
use std::path::PathBuf;
use tempfile::TempDir;

const STAGE: &str = "s1";

struct Fixture {
    _tmp: TempDir,
    work_dir: PathBuf,
    worktree: PathBuf,
    stage: Stage,
}

/// A v2 standard stage, its work dir, and its worktree on `loom/s1` off
/// `main` with one untracked change.
fn fixture() -> Fixture {
    let tmp = TempDir::new().unwrap();
    let work_dir = tmp.path().join("work");
    std::fs::create_dir_all(&work_dir).unwrap();
    let worktree = contract_worktree(&tmp.path().join("repo"), STAGE);
    let stage = Stage {
        id: STAGE.to_string(),
        plan_version: 2,
        stage_type: StageType::Standard,
        ..Stage::default()
    };
    Fixture {
        _tmp: tmp,
        work_dir,
        worktree,
        stage,
    }
}

fn finding(claim: &str) -> Finding {
    Finding {
        severity: "major".to_string(),
        file: "src/a.rs".to_string(),
        line: 42,
        claim: claim.to_string(),
        scenario: Some("empty input → panic".to_string()),
        rule: None,
    }
}

fn round(number: u32, at: &ChangeFingerprint, claims: &[&str], resolved: &[&str]) -> ReviewRound {
    let findings = claims
        .iter()
        .enumerate()
        .map(|(index, claim)| RecordedFinding {
            id: finding_id(number, index + 1),
            finding: finding(claim),
        });
    ReviewRound {
        version: RECORD_VERSION,
        round: number,
        agent_id: format!("agent-{number}"),
        harvested_at: chrono::Utc::now(),
        fingerprint: at.value.clone(),
        files: at.files.clone(),
        malformed: None,
        findings: findings.collect(),
        resolved: resolved.iter().map(|id| id.to_string()).collect(),
        unresolved: Vec::new(),
        suggestion_memory_ids: Vec::new(),
    }
}

fn current(fx: &Fixture) -> ChangeFingerprint {
    fingerprint::compute(&fx.worktree, "main").unwrap()
}

fn gate(fx: &Fixture) -> Result<()> {
    check(&fx.stage, &fx.work_dir, &fx.worktree, "main")
}

fn write_record(fx: &Fixture, file: &str, json: &str) {
    let dir = fx.work_dir.join("reviews").join(STAGE);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(file), json).unwrap();
}

/// One `carried.json` entry in the DESIGN D12 shape.
fn carried_json(id: &str) -> String {
    format!(
        r#"{{ "id": "{id}", "origin_stage": "origin", "dispute": 2, "finding": {{
            "severity": "minor", "file": "src/b.rs", "line": 7, "claim": "c",
            "scenario": "s", "rule": null }} }}"#
    )
}

#[test]
fn gate_fails_without_review_at_current_fingerprint() {
    let fx = fixture();
    write_round(&fx.work_dir, STAGE, &round(1, &current(&fx), &[], &[])).unwrap();
    gate(&fx).unwrap();

    std::fs::write(
        fx.worktree.join(CONTRACT_FILE),
        "changed after the review\n",
    )
    .unwrap();

    let error = gate(&fx).unwrap_err().to_string();
    assert!(error.contains("loom stage review status s1"), "{error}");
    assert!(error.contains("review round 1 saw"), "{error}");
    assert!(error.contains(CONTRACT_FILE), "{error}");
}

#[test]
fn gate_fails_with_open_finding() {
    let fx = fixture();
    let seen = current(&fx);
    write_round(
        &fx.work_dir,
        STAGE,
        &round(1, &seen, &["unwrap on input"], &[]),
    )
    .unwrap();

    let error = gate(&fx).unwrap_err().to_string();
    assert!(
        error.contains("open finding F-1-1 (major) src/a.rs:42: unwrap on input"),
        "{error}"
    );
    assert!(
        error.contains("fix them and run a re-review, or dispute them"),
        "{error}"
    );
    assert!(error.contains("loom stage review status s1"), "{error}");
    assert!(!error.contains(" saw "), "the review is current: {error}");
}

#[test]
fn gate_flattens_reviewer_text_in_its_message() {
    let fx = fixture();
    let claim = "first line\nsecond \u{1b}[31mred";
    write_round(&fx.work_dir, STAGE, &round(1, &current(&fx), &[claim], &[])).unwrap();

    let error = gate(&fx).unwrap_err().to_string();
    assert!(!error.contains('\u{1b}'), "{error:?}");
    assert!(
        error.contains("open finding F-1-1 (major) src/a.rs:42: first line second [31mred\n"),
        "{error:?}"
    );
}

#[test]
fn gate_passes_when_later_round_resolves_finding() {
    let fx = fixture();
    let earlier = ChangeFingerprint {
        value: "sha256:earlier".to_string(),
        base: "a".repeat(40),
        files: BTreeMap::new(),
    };
    write_round(
        &fx.work_dir,
        STAGE,
        &round(1, &earlier, &["unwrap on input"], &[]),
    )
    .unwrap();
    write_round(
        &fx.work_dir,
        STAGE,
        &round(2, &current(&fx), &[], &["F-1-1"]),
    )
    .unwrap();

    gate(&fx).unwrap();
}

#[test]
fn a_malformed_latest_round_is_quoted_and_is_not_a_review() {
    let fx = fixture();
    let mut malformed = round(1, &current(&fx), &[], &[]);
    malformed.malformed = Some("no loom-review block".to_string());
    write_round(&fx.work_dir, STAGE, &malformed).unwrap();

    let error = gate(&fx).unwrap_err().to_string();
    assert!(
        error.contains("no well-formed review round is recorded"),
        "{error}"
    );
    assert!(
        error.contains("round (1) is malformed: \"no loom-review block\""),
        "{error}"
    );
}

#[test]
fn only_v2_standard_and_integration_verify_stages_are_gated() {
    let fx = fixture();
    let v1 = Stage {
        plan_version: 1,
        ..fx.stage.clone()
    };
    check_at_completion(&v1, &fx.work_dir, None, "main").unwrap();
    let knowledge = Stage {
        stage_type: StageType::Knowledge,
        ..fx.stage.clone()
    };
    check_at_completion(&knowledge, &fx.work_dir, None, "main").unwrap();

    let error = check_at_completion(&fx.stage, &fx.work_dir, None, "main").unwrap_err();
    assert!(error.to_string().contains("no worktree"), "{error}");
    let iv = Stage {
        stage_type: StageType::IntegrationVerify,
        ..fx.stage.clone()
    };
    let worktree = Some(fx.worktree.as_path());
    let error = check_at_completion(&iv, &fx.work_dir, worktree, "main").unwrap_err();
    assert!(
        error.to_string().contains("no well-formed review round"),
        "{error}"
    );
}

#[test]
fn rulings_and_carried_findings_decide_what_stays_open() {
    let fx = fixture();
    let seen = current(&fx);
    let claims = ["dismissed", "upheld", "resolved by itself"];
    let own = round(1, &seen, &claims, &["F-1-3", "origin/F-2-1"]);
    write_round(&fx.work_dir, STAGE, &own).unwrap();
    write_record(
        &fx,
        "rulings.json",
        r#"{ "version": 1, "rulings": [
            { "finding": "F-1-1", "ruling": "dismiss", "target_stage": null, "dispute": 3 },
            { "finding": "F-1-2", "ruling": "uphold", "target_stage": null, "dispute": 3 },
            { "finding": "origin/F-1-4", "ruling": "defer", "target_stage": "later", "dispute": 4 }
        ] }"#,
    );
    let entries = ["origin/F-1-1", "origin/F-2-1", "origin/F-1-4"].map(carried_json);
    let json = format!(r#"{{ "version": 1, "carried": [{}] }}"#, entries.join(","));
    write_record(&fx, "carried.json", &json);

    let open = open_findings(&fx.work_dir, STAGE).unwrap();
    let ids: Vec<&str> = open.iter().map(|finding| finding.id.as_str()).collect();
    assert_eq!(ids, ["F-1-2", "F-1-3", "origin/F-1-1"]);
    assert_eq!(open[0].origin_stage, None);
    assert_eq!(open[2].origin_stage.as_deref(), Some("origin"));
}

#[test]
fn rounds_are_numbered_in_order_and_never_replaced() {
    let tmp = TempDir::new().unwrap();
    let work_dir = tmp.path();
    let seen = ChangeFingerprint {
        value: "sha256:x".to_string(),
        base: "b".to_string(),
        files: BTreeMap::from([("src/a.rs".to_string(), "deleted".to_string())]),
    };
    assert_eq!(next_round(work_dir, STAGE).unwrap(), 1);
    write_round(work_dir, STAGE, &round(1, &seen, &["c"], &[])).unwrap();
    assert_eq!(next_round(work_dir, STAGE).unwrap(), 2);

    let again = write_round(work_dir, STAGE, &round(1, &seen, &[], &[])).unwrap_err();
    assert!(again.to_string().contains("already recorded"), "{again:#}");
    let mut misnumbered = round(2, &seen, &["c"], &[]);
    misnumbered.findings[0].id = "F-1-1".to_string();
    assert!(write_round(work_dir, STAGE, &misnumbered).is_err());
    assert!(write_round(work_dir, "../escape", &round(3, &seen, &[], &[])).is_err());

    let raw = std::fs::read_to_string(work_dir.join("reviews/s1/round-1.json")).unwrap();
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(json["findings"][0]["id"], "F-1-1");
    assert_eq!(json["findings"][0]["severity"], "major");
    assert_eq!(json["malformed"], serde_json::Value::Null);
    assert_eq!(json["files"]["src/a.rs"], "deleted");
}
