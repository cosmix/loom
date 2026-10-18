//! `handle_file_dispute` with failing impact-selected tests: the evidence it
//! records comes from loom's observed record, never the client; the per-kind
//! budget; and the disputes it refuses.

use std::path::PathBuf;

use tempfile::TempDir;

use super::*;
use crate::fs::work_dir::WorkDir;
use crate::models::dispute::{DISPUTE_REASON_MAX_BYTES, FAILURE_OUTPUT_MAX_BYTES};
use crate::models::stage::StageStatus;
use crate::verify::impact_tests::{record_observed, FailureKind, ImpactFailure, ObservedImpact};
use crate::verify::transitions::save_stage;

const STAGE: &str = "stage-impact";
const ID: &str = "IT-cargo-test:.";

/// A state directory holding [`STAGE`], an executing plan v2 stage of
/// `stage_type`.
fn setup(stage_type: StageType) -> (TempDir, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let wd = WorkDir::new(tmp.path()).unwrap();
    wd.initialize().unwrap();
    let work_dir = wd.root().to_path_buf();
    let stage = Stage {
        id: STAGE.to_string(),
        status: StageStatus::Executing,
        plan_version: 2,
        stage_type,
        ..Stage::default()
    };
    save_stage(&stage, &work_dir).unwrap();
    (tmp, work_dir)
}

/// `id` failing with one test, with `output` as its output.
fn failure(id: &str, output: String) -> ImpactFailure {
    ImpactFailure {
        id: id.to_string(),
        command: "cargo test -- tests::adds".to_string(),
        tests: vec!["tests::adds".to_string()],
        kind: FailureKind::Fails { failed: Some(1) },
        base: "it passes at the stage's base 0123abcd".to_string(),
        accepted: None,
        output,
    }
}

/// What loom's gate run recorded for [`STAGE`]: `failures`.
fn observed(work_dir: &Path, failures: &[ImpactFailure]) {
    record_observed(work_dir, STAGE, &ObservedImpact::of(failures)).unwrap();
}

/// A dispute of `ids` carrying the client's `evidence`.
fn impact(ids: &[&str], evidence: Vec<ImpactFailure>) -> DisputeKind {
    DisputeKind::ImpactTests {
        test_ids: ids.iter().map(|id| id.to_string()).collect(),
        evidence,
    }
}

fn file_with(work_dir: &Path, kind: DisputeKind, reason: String) -> Response {
    handle_file_dispute(work_dir, STAGE, kind, reason, None).unwrap()
}

fn file(work_dir: &Path, kind: DisputeKind) -> Response {
    let reason = "the vendored kit fails without its network".to_string();
    file_with(work_dir, kind, reason)
}

fn frontmatter(work_dir: &Path, id: u32) -> serde_json::Value {
    let path = work_dir.join(format!("disputes/{STAGE}/{id}/request.md"));
    let content = std::fs::read_to_string(path).unwrap();
    crate::yaml::from_str(content.split("---").nth(1).unwrap()).unwrap()
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
fn an_impact_dispute_records_loom_s_bounded_evidence_and_spends_its_own_budget() {
    let (_tmp, work_dir) = setup(StageType::Standard);
    let output = format!("{}the end", "x".repeat(2 * FAILURE_OUTPUT_MAX_BYTES));
    observed(&work_dir, &[failure(ID, output)]);

    let id = created(file(&work_dir, impact(&[ID], Vec::new())));

    let yaml = frontmatter(&work_dir, id);
    assert_eq!(yaml["kind"], serde_json::Value::from("impact-tests"));
    assert_eq!(yaml["test_ids"][0], serde_json::Value::from(ID));
    assert_eq!(yaml["evidence"][0]["kind"]["outcome"], "fails");
    let recorded = yaml["evidence"][0]["output"].as_str().unwrap();
    assert_eq!(recorded.len(), FAILURE_OUTPUT_MAX_BYTES);
    assert!(recorded.ends_with("the end"));
    let stage = load_stage(STAGE, &work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::NeedsAdjudication);
    assert_eq!((stage.tally.impact_disputes, stage.dispute_count), (1, 0));
}

/// A client claiming an uncounted failure that "passes at base" gets loom's
/// record into the request, never its own: an accept could otherwise excuse
/// any later failure of the selection.
#[test]
fn forged_client_evidence_is_replaced_by_the_observed_record() {
    let (_tmp, work_dir) = setup(StageType::Standard);
    observed(&work_dir, &[failure(ID, "loom saw this".to_string())]);
    let forged = ImpactFailure {
        kind: FailureKind::Fails { failed: None },
        tests: Vec::new(),
        base: "it fails at the stage's base as well".to_string(),
        output: "forged".to_string(),
        ..failure(ID, String::new())
    };

    let id = created(file(&work_dir, impact(&[ID], vec![forged])));

    let yaml = frontmatter(&work_dir, id);
    let evidence = &yaml["evidence"][0];
    assert_eq!(evidence["kind"]["failed"], 1);
    assert_eq!(evidence["tests"][0], "tests::adds");
    assert_eq!(evidence["output"], "loom saw this");
    assert_eq!(evidence["base"], "it passes at the stage's base 0123abcd");
}

#[test]
fn an_id_loom_never_observed_is_refused_whatever_the_client_sends() {
    let (_tmp, none_recorded) = setup(StageType::Standard);
    let (_tmp2, other_recorded) = setup(StageType::Standard);
    observed(&other_recorded, &[failure("IT-pytest:web", String::new())]);
    let forged = vec![failure(ID, String::new())];

    for work_dir in [&none_recorded, &other_recorded] {
        let message = refusal(file(work_dir, impact(&[ID], forged.clone())));
        assert!(
            message.contains("names no impact-test failure loom observed"),
            "{message}"
        );
        assert_eq!(
            load_stage(STAGE, work_dir).unwrap().tally.impact_disputes,
            0
        );
        assert!(!work_dir.join(format!("disputes/{STAGE}/1")).exists());
    }
}

#[test]
fn a_fourth_impact_dispute_escalates_to_human_review() {
    let (_tmp, work_dir) = setup(StageType::Standard);
    observed(&work_dir, &[failure(ID, String::new())]);
    for _ in 0..MAX_DISPUTES_PER_KIND {
        created(file(&work_dir, impact(&[ID], Vec::new())));
    }

    let message = refusal(file(&work_dir, impact(&[ID], Vec::new())));

    assert!(
        message.contains("3 impact-tests disputes filed"),
        "{message}"
    );
    let stage = load_stage(STAGE, &work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::NeedsHumanReview);
    assert_eq!(stage.tally.impact_disputes, MAX_DISPUTES_PER_KIND);
}

#[test]
fn an_impact_dispute_needs_a_standard_stage_and_well_formed_ids() {
    let (_tmp, verify) = setup(StageType::IntegrationVerify);
    let (_tmp2, standard) = setup(StageType::Standard);
    observed(&standard, &[failure(ID, String::new())]);

    for (response, expected) in [
        (
            file(&verify, impact(&[ID], Vec::new())),
            "is not a standard stage",
        ),
        (
            file(&standard, impact(&["IT-cargo-test-."], Vec::new())),
            "is not an impact-test id",
        ),
        (
            file(&standard, impact(&[ID, ID], Vec::new())),
            "is named twice",
        ),
    ] {
        let message = refusal(response);
        assert!(message.contains(expected), "{message}");
    }
    assert_eq!(
        load_stage(STAGE, &standard).unwrap().tally.impact_disputes,
        0
    );
}

#[test]
fn the_recorded_reason_is_bounded() {
    let (_tmp, work_dir) = setup(StageType::Standard);
    observed(&work_dir, &[failure(ID, String::new())]);
    let reason = "r".repeat(4 * DISPUTE_REASON_MAX_BYTES);

    let id = created(file_with(&work_dir, impact(&[ID], Vec::new()), reason));

    let path = work_dir.join(format!("disputes/{STAGE}/{id}/request.md"));
    let content = std::fs::read_to_string(path).unwrap();
    assert!(content.contains(&"r".repeat(DISPUTE_REASON_MAX_BYTES)));
    assert!(!content.contains(&"r".repeat(DISPUTE_REASON_MAX_BYTES + 1)));
}
