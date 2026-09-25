//! Applying findings, contract and integrity verdicts (`apply_kinds.rs`),
//! each driven through `AdjudicatorRegistry::apply_verdict` from the files the
//! daemon reads: the stage, `request.md` and `verdict.md`.

use chrono::Utc;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

use super::tests::{make_stage, reject_verdict, write_request, write_stage, write_verdict};
use super::{feedback, AdjudicatorRegistry};
use crate::fs::work_dir::WorkDir;
use crate::models::dispute::{
    Citation, DisputeKind, DisputeVerdict, FindingRuling, FindingSnapshot, PlanPatch,
};
use crate::models::stage::{Stage, StageStatus, StageType};
use crate::relay::sha256_hex;
use crate::verify::contracts::store::{self as contract_store, FreezeRecord, FrozenFile};
use crate::verify::contracts::test_support::{
    contract, contract_worktree, CONTRACT_CONTENT, CONTRACT_FILE, CONTRACT_ID,
};
use crate::verify::integrity::{self, EventKind, IntegrityEvent};
use crate::verify::review::report::Finding;
use crate::verify::review::store::{self, RecordedFinding, ReviewRound, RulingKind};
use crate::verify::transitions::load_stage;

fn work_dir() -> TempDir {
    let tmp = TempDir::new().unwrap();
    std::fs::create_dir_all(tmp.path().join("stages")).unwrap();
    tmp
}

fn apply(work: &Path, stage_id: &str) {
    AdjudicatorRegistry::new()
        .apply_verdict(work, stage_id, 1)
        .unwrap();
}

fn status(work: &Path, stage_id: &str) -> StageStatus {
    load_stage(stage_id, work).unwrap().status
}

fn citations() -> Vec<Citation> {
    vec![Citation {
        file: "src/a.rs".to_string(),
        line: Some(4),
        excerpt: "let x = y;".to_string(),
        claim: "the finding's scenario cannot occur".to_string(),
    }]
}

fn accept_without_patch() -> DisputeVerdict {
    DisputeVerdict::Accept {
        plan_patch: PlanPatch { inner: json!({}) },
        citations: citations(),
        reasoning: "the disputed change is sound".to_string(),
    }
}

fn finding() -> Finding {
    Finding {
        severity: "major".to_string(),
        file: "src/a.rs".to_string(),
        line: 4,
        claim: "leaks the handle".to_string(),
        scenario: Some("early return → handle never closed".to_string()),
        rule: None,
    }
}

/// Save `stage` with one harvested review round holding finding `F-1-1`.
fn stage_with_finding(work: &Path, stage: &Stage) {
    write_stage(work, stage);
    let round = ReviewRound {
        version: store::RECORD_VERSION,
        round: 1,
        agent_id: "reviewer".to_string(),
        harvested_at: Utc::now(),
        fingerprint: "sha256:0".to_string(),
        files: BTreeMap::new(),
        malformed: None,
        findings: vec![RecordedFinding {
            id: "F-1-1".to_string(),
            finding: finding(),
        }],
        resolved: Vec::new(),
        unresolved: Vec::new(),
        suggestion_memory_ids: Vec::new(),
    };
    store::write_round(work, &stage.id, &round).unwrap();
}

/// Dispute 1 of `stage_id` contests `F-1-1`, and its verdict rules `ruling`.
fn rule_on_finding(work: &Path, stage_id: &str, ruling: RulingKind, target: Option<&str>) {
    let evidence = FindingSnapshot {
        id: "F-1-1".to_string(),
        origin_stage: None,
        round: 1,
        finding: finding(),
    };
    let kind = DisputeKind::Findings {
        finding_ids: vec![evidence.id.clone()],
        evidence: vec![evidence],
    };
    write_request(work, stage_id, 1, kind);
    let rulings = vec![FindingRuling {
        finding: "F-1-1".to_string(),
        ruling,
        target_stage: target.map(str::to_string),
        reasoning: "judged on the cited lines".to_string(),
        citations: citations(),
    }];
    write_verdict(work, stage_id, 1, DisputeVerdict::Rulings { rulings }, 1);
}

fn open_ids(work: &Path, stage_id: &str) -> Vec<String> {
    let open = store::open_findings(work, stage_id).unwrap();
    open.into_iter().map(|finding| finding.id).collect()
}

#[test]
fn dismiss_ruling_closes_finding() {
    let tmp = work_dir();
    let work = tmp.path();
    stage_with_finding(work, &make_stage("s1"));
    assert_eq!(open_ids(work, "s1"), ["F-1-1"]);
    rule_on_finding(work, "s1", RulingKind::Dismiss, None);

    apply(work, "s1");

    assert!(open_ids(work, "s1").is_empty());
    assert_eq!(status(work, "s1"), StageStatus::Queued);
}

#[test]
fn defer_ruling_carries_finding_to_target() {
    let tmp = work_dir();
    let work = tmp.path();
    stage_with_finding(work, &make_stage("s1"));
    let target = Stage {
        status: StageStatus::WaitingForDeps,
        dependencies: vec!["s1".to_string()],
        ..make_stage("s2")
    };
    write_stage(work, &target);
    rule_on_finding(work, "s1", RulingKind::Defer, Some("s2"));

    apply(work, "s1");

    let carried = store::load_carried(work, "s2").unwrap().carried;
    assert_eq!(carried.len(), 1);
    assert_eq!(carried[0].id, "s1/F-1-1");
    assert_eq!(carried[0].origin_stage, "s1");
    assert_eq!(open_ids(work, "s2"), ["s1/F-1-1"]);
    assert!(open_ids(work, "s1").is_empty());
    assert_eq!(status(work, "s1"), StageStatus::Queued);
}

#[test]
fn defer_from_integration_verify_is_coerced() {
    let tmp = work_dir();
    let work = tmp.path();
    let verify = Stage {
        stage_type: StageType::IntegrationVerify,
        ..make_stage("iv")
    };
    stage_with_finding(work, &verify);
    rule_on_finding(work, "iv", RulingKind::Defer, Some("s2"));

    apply(work, "iv");

    assert!(store::load_rulings(work, "iv").unwrap().rulings.is_empty());
    assert_eq!(open_ids(work, "iv"), ["F-1-1"]);
    let asked = feedback::read_feedback(work, "iv").unwrap().unwrap();
    assert!(asked.contains("integration-verify never defers"), "{asked}");
    assert_eq!(load_stage("iv", work).unwrap().tally.evidence_rounds, 1);
}

/// A repository whose stage worktree holds the contract file, a state
/// directory beside it, and the stage under adjudication with its contract
/// frozen at [`CONTRACT_CONTENT`].
fn frozen_contract_fixture() -> (TempDir, PathBuf, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    let worktree = contract_worktree(&repo, "s1");
    let workspace = WorkDir::new(&repo).unwrap();
    workspace.adopt_existing().unwrap();
    let work = workspace.root().to_path_buf();
    let stage = Stage {
        plan_version: 2,
        worktree: Some("s1".to_string()),
        contracts: vec![contract()],
        ..make_stage("s1")
    };
    write_stage(&work, &stage);
    let record = FreezeRecord {
        version: contract_store::FREEZE_RECORD_VERSION,
        stage_id: "s1".to_string(),
        session_id: "contract-session".to_string(),
        frozen_at: Utc::now(),
        base: "main".to_string(),
        files: vec![FrozenFile {
            path: CONTRACT_FILE.to_string(),
            sha256: sha256_hex(CONTRACT_CONTENT),
        }],
        contracts: Vec::new(),
    };
    let copies = [(CONTRACT_FILE.to_string(), CONTRACT_CONTENT.to_vec())];
    contract_store::write_freeze(&work, &record, &copies).unwrap();
    (tmp, work, worktree)
}

#[test]
fn contract_accept_refreezes_current_content() {
    let (_tmp, work, worktree) = frozen_contract_fixture();
    let edited: &[u8] = b"#[test]\nfn rejects_x() { assert!(reject(\"x\")); }\n";
    std::fs::write(worktree.join(CONTRACT_FILE), edited).unwrap();
    let kind = DisputeKind::Contract {
        contract_id: CONTRACT_ID.to_string(),
    };
    write_request(&work, "s1", 1, kind);
    write_verdict(&work, "s1", 1, accept_without_patch(), 1);

    apply(&work, "s1");

    let record = contract_store::load_freeze(&work, "s1").unwrap().unwrap();
    assert_eq!(record.files.len(), 1);
    assert_eq!(record.files[0].sha256, sha256_hex(edited));
    let copy = contract_store::frozen_file_path(&work, "s1", CONTRACT_FILE);
    assert_eq!(std::fs::read(copy).unwrap(), edited);
    assert_eq!(status(&work, "s1"), StageStatus::Queued);
}

#[test]
fn contract_reject_requeues_with_restore_feedback() {
    let tmp = work_dir();
    let work = tmp.path();
    write_stage(work, &make_stage("s1"));
    let kind = DisputeKind::Contract {
        contract_id: "rejects-x".to_string(),
    };
    write_request(work, "s1", 1, kind);
    write_verdict(work, "s1", 1, reject_verdict(), 1);

    apply(work, "s1");

    assert_eq!(status(work, "s1"), StageStatus::Queued);
    let notice = feedback::read_feedback(work, "s1").unwrap().unwrap();
    assert!(
        notice.contains("loom stage contracts restore s1 --contract rejects-x"),
        "{notice}"
    );
}

#[test]
fn integrity_accept_records_counts() {
    let tmp = work_dir();
    let work = tmp.path();
    write_stage(work, &make_stage("s1"));
    let event = IntegrityEvent {
        id: "TI-assert-rust".to_string(),
        kind: EventKind::AssertTotal,
        language: Some("rust".to_string()),
        path: None,
        base: Some(120),
        current: Some(118),
        current_sha256: None,
        detail: Vec::new(),
    };
    let kind = DisputeKind::Integrity {
        event_ids: vec![event.id.clone()],
        evidence: vec![event],
    };
    write_request(work, "s1", 1, kind);
    write_verdict(work, "s1", 1, accept_without_patch(), 1);

    apply(work, "s1");

    let accepted = integrity::load_accepted(work, "s1").unwrap().accepted;
    assert_eq!(accepted.len(), 1);
    assert_eq!(accepted[0].event, "TI-assert-rust");
    assert_eq!(accepted[0].base, Some(120));
    assert_eq!(accepted[0].accepted_current, Some(118));
    assert_eq!(accepted[0].dispute, 1);
    assert_eq!(status(work, "s1"), StageStatus::Queued);
}
