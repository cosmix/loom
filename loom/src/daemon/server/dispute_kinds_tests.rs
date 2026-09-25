//! `handle_file_dispute` against a real state directory, and the criterion
//! path's `request.md` beside it.

use std::collections::BTreeMap;
use std::path::PathBuf;

use tempfile::TempDir;

use super::*;
use crate::daemon::handle_dispute_criteria;
use crate::fs::work_dir::WorkDir;
use crate::models::dispute::FindingSnapshot;
use crate::models::stage::StageStatus;
use crate::plan::schema::AcceptanceCriterion;
use crate::verify::contracts::store::{write_freeze, FreezeRecord, FrozenContract};
use crate::verify::contracts::test_support::{red_reports, CONTRACT_ID};
use crate::verify::review::report::Finding;
use crate::verify::review::store::{write_round, RecordedFinding, ReviewRound, RECORD_VERSION};
use crate::verify::transitions::save_stage;

const STAGE: &str = "stage-disp";
const CLAIM: &str = "the parser accepts an empty list";

/// A state directory holding [`STAGE`], executing, built from a plan of
/// `plan_version`, with two acceptance criteria.
fn setup(plan_version: u32) -> (TempDir, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let wd = WorkDir::new(tmp.path()).unwrap();
    wd.initialize().unwrap();
    let work_dir = wd.root().to_path_buf();
    let stage = Stage {
        id: STAGE.to_string(),
        name: "Disp".to_string(),
        status: StageStatus::Executing,
        plan_version,
        acceptance: vec![
            AcceptanceCriterion::Simple("echo 0".to_string()),
            AcceptanceCriterion::Simple("echo 1".to_string()),
        ],
        ..Stage::default()
    };
    save_stage(&stage, &work_dir).unwrap();
    (tmp, work_dir)
}

/// Review round 1 of [`STAGE`], raising the one finding `F-1-1`.
fn record_open_finding(work_dir: &Path) {
    let round = ReviewRound {
        version: RECORD_VERSION,
        round: 1,
        agent_id: "reviewer-1".to_string(),
        harvested_at: Utc::now(),
        fingerprint: "sha256:00".to_string(),
        files: BTreeMap::new(),
        malformed: None,
        findings: vec![RecordedFinding {
            id: "F-1-1".to_string(),
            finding: Finding {
                severity: "high".to_string(),
                file: "src/parse.rs".to_string(),
                line: 12,
                claim: CLAIM.to_string(),
                scenario: Some("an empty list".to_string()),
                rule: None,
            },
        }],
        resolved: Vec::new(),
        unresolved: Vec::new(),
        suggestion_memory_ids: Vec::new(),
    };
    write_round(work_dir, STAGE, &round).unwrap();
}

fn findings(evidence: Vec<FindingSnapshot>) -> DisputeKind {
    DisputeKind::Findings {
        finding_ids: vec!["F-1-1".to_string()],
        evidence,
    }
}

fn file(work_dir: &Path, kind: DisputeKind) -> Response {
    handle_file_dispute(work_dir, STAGE, kind, "wrong".to_string(), None).unwrap()
}

fn request_path(work_dir: &Path, id: u32) -> PathBuf {
    work_dir.join(format!("disputes/{STAGE}/{id}/request.md"))
}

/// The frontmatter of dispute `id`'s `request.md`.
fn frontmatter(work_dir: &Path, id: u32) -> serde_yaml::Value {
    let content = std::fs::read_to_string(request_path(work_dir, id)).unwrap();
    serde_yaml::from_str(content.split("---").nth(1).unwrap()).unwrap()
}

fn created(response: Response) -> u32 {
    match response {
        Response::DisputeCreated { id } => id,
        other => panic!("expected a filed dispute, got {other:?}"),
    }
}

fn refusal(response: Response) -> String {
    match response {
        Response::Error { message } => message,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn finding_dispute_records_kind_and_evidence() {
    let (_tmp, work_dir) = setup(2);
    record_open_finding(&work_dir);

    let id = created(file(&work_dir, findings(Vec::new())));

    let yaml = frontmatter(&work_dir, id);
    assert_eq!(yaml["kind"], serde_yaml::Value::from("findings"));
    assert_eq!(yaml["finding_ids"][0], serde_yaml::Value::from("F-1-1"));
    assert_eq!(yaml["evidence"][0]["id"], serde_yaml::Value::from("F-1-1"));
    assert_eq!(yaml["evidence"][0]["round"], serde_yaml::Value::from(1));
    assert_eq!(
        yaml["evidence"][0]["finding"]["claim"],
        serde_yaml::Value::from(CLAIM)
    );
    let stage = load_stage(STAGE, &work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::NeedsAdjudication);
    assert_eq!((stage.tally.finding_disputes, stage.dispute_count), (1, 0));
}

#[test]
fn finding_dispute_budget_escalates_to_human_review() {
    let (_tmp, work_dir) = setup(2);
    record_open_finding(&work_dir);
    for expected in 1..=MAX_DISPUTES_PER_KIND {
        assert_eq!(created(file(&work_dir, findings(Vec::new()))), expected);
    }

    let message = refusal(file(&work_dir, findings(Vec::new())));

    assert!(message.contains("Dispute budget exhausted"), "{message}");
    assert!(!request_path(&work_dir, MAX_DISPUTES_PER_KIND + 1).exists());
    let stage = load_stage(STAGE, &work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::NeedsHumanReview);
    assert_eq!(stage.tally.finding_disputes, MAX_DISPUTES_PER_KIND);
    let review_reason = stage.review_reason.unwrap_or_default();
    assert!(
        review_reason.contains("findings disputes filed"),
        "{review_reason}"
    );
}

#[test]
fn criterion_dispute_behaviour_is_unchanged() {
    let (_tmp, work_dir) = setup(1);

    let response = handle_dispute_criteria(
        &work_dir,
        STAGE,
        1,
        "criterion 1 is unrunnable".to_string(),
        Some("abc1234".to_string()),
        Some("exit 127".to_string()),
    )
    .unwrap();
    let id = created(response);

    let yaml = frontmatter(&work_dir, id);
    let mut keys: Vec<&str> = yaml
        .as_mapping()
        .unwrap()
        .keys()
        .map(|key| key.as_str().unwrap())
        .collect();
    keys.sort_unstable();
    let expected = [
        "created_at",
        "criterion_index",
        "evidence_commit",
        "failure_output",
        "fix_attempts_at_dispute",
        "id",
        "kind",
        "reason",
        "stage_id",
    ];
    assert_eq!(keys, expected);
    assert_eq!(yaml["kind"], serde_yaml::Value::from("criterion"));
    assert_eq!(yaml["criterion_index"], serde_yaml::Value::from(1));
    let content = std::fs::read_to_string(request_path(&work_dir, id)).unwrap();
    assert!(content.ends_with(&format!("# Dispute request {id} for stage {STAGE}\n")));
    let stage = load_stage(STAGE, &work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::NeedsAdjudication);
    assert_eq!((stage.dispute_count, stage.tally.evidence_rounds), (1, 0));
    assert_eq!(
        stage.review_reason.as_deref(),
        Some("criterion 1 is unrunnable")
    );
}

#[test]
fn the_daemon_records_its_own_evidence_not_the_callers() {
    let (_tmp, work_dir) = setup(2);
    record_open_finding(&work_dir);
    let open = open_findings(&work_dir, STAGE).unwrap();
    let mut forged = select_findings(&open, &["F-1-1".to_string()]).unwrap();
    forged[0].finding.claim = "a finding nobody raised".to_string();

    let id = created(file(&work_dir, findings(forged)));

    let yaml = frontmatter(&work_dir, id);
    assert_eq!(
        yaml["evidence"][0]["finding"]["claim"],
        serde_yaml::Value::from(CLAIM)
    );
}

#[test]
fn an_id_that_does_not_exist_is_refused_before_anything_is_written() {
    let (_tmp, work_dir) = setup(2);
    record_open_finding(&work_dir);
    let unknown = DisputeKind::Findings {
        finding_ids: vec!["F-9-9".to_string()],
        evidence: Vec::new(),
    };
    let contract = DisputeKind::Contract {
        contract_id: CONTRACT_ID.to_string(),
    };

    let not_open = refusal(file(&work_dir, unknown));
    let not_frozen = refusal(file(&work_dir, contract));

    assert!(
        not_open.contains("names no open review finding"),
        "{not_open}"
    );
    assert!(not_frozen.contains("are not frozen"), "{not_frozen}");
    assert!(!request_path(&work_dir, 1).exists());
    let stage = load_stage(STAGE, &work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::Executing);
}

#[test]
fn a_frozen_contract_can_be_disputed() {
    let (_tmp, work_dir) = setup(2);
    let record = FreezeRecord {
        version: crate::verify::contracts::store::FREEZE_RECORD_VERSION,
        stage_id: STAGE.to_string(),
        session_id: "session-contract".to_string(),
        frozen_at: Utc::now(),
        base: "main".to_string(),
        files: Vec::new(),
        contracts: red_reports().iter().map(FrozenContract::from).collect(),
    };
    write_freeze(&work_dir, &record, &[]).unwrap();
    let contract = DisputeKind::Contract {
        contract_id: CONTRACT_ID.to_string(),
    };

    let id = created(file(&work_dir, contract));

    assert_eq!(
        frontmatter(&work_dir, id)["contract_id"],
        serde_yaml::Value::from(CONTRACT_ID)
    );
    let stage = load_stage(STAGE, &work_dir).unwrap();
    assert_eq!(stage.tally.contract_disputes, 1);
}

#[test]
fn a_plan_version_1_stage_disputes_only_its_criteria() {
    let (_tmp, work_dir) = setup(1);
    record_open_finding(&work_dir);

    let message = refusal(file(&work_dir, findings(Vec::new())));

    assert!(message.contains("not a plan version 2 stage"), "{message}");
    assert!(!request_path(&work_dir, 1).exists());
    assert_eq!(
        load_stage(STAGE, &work_dir).unwrap().tally.finding_disputes,
        0
    );
}
