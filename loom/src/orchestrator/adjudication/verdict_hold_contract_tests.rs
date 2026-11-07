//! Contracts for which verdicts hold a stage for the daemon's completion run
//! (`NeedsAdjudication`, no session) and which still re-queue it, each driven
//! through `AdjudicatorRegistry::apply_verdict` from the files the daemon reads.

use chrono::Utc;
use serde_json::json;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

use super::tests::{make_stage, write_request, write_stage, write_verdict};
use super::{is_held_for_completion, AdjudicatorRegistry};
use crate::fs::work_dir::WorkDir;
use crate::models::dispute::{
    Citation, DisputeKind, DisputeVerdict, FindingRuling, FindingSnapshot, PlanPatch,
};
use crate::models::stage::{Stage, StageStatus};
use crate::relay::sha256_hex;
use crate::verify::contracts::store::{
    self as contract_store, FreezeRecord, FrozenContract, FrozenFile,
};
use crate::verify::contracts::test_support::{
    contract, contract_worktree, red_reports, CONTRACT_CONTENT, CONTRACT_FILE, CONTRACT_ID,
};
use crate::verify::integrity::{EventKind, IntegrityEvent};
use crate::verify::review::store::RulingKind;
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

fn citations() -> Vec<Citation> {
    vec![Citation {
        file: "src/a.rs".to_string(),
        line: Some(4),
        excerpt: "let x = y;".to_string(),
        claim: "the disputed change is sound".to_string(),
    }]
}

fn accept_without_patch() -> DisputeVerdict {
    DisputeVerdict::Accept {
        plan_patch: PlanPatch { inner: json!({}) },
        citations: citations(),
        reasoning: "the disputed change is sound".to_string(),
    }
}

#[test]
fn accepted_integrity_verdict_holds_the_stage() {
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

    let stage = load_stage("s1", work).unwrap();
    assert_eq!(stage.status, StageStatus::NeedsAdjudication);
    assert_eq!(stage.session, None);
    assert!(work.join("disputes/s1/1/applied.marker").exists());
    assert!(is_held_for_completion(work, &stage));
}

fn ruling(finding: &str, ruling: RulingKind) -> FindingRuling {
    FindingRuling {
        finding: finding.to_string(),
        ruling,
        target_stage: None,
        reasoning: "judged on the cited lines".to_string(),
        citations: citations(),
    }
}

#[test]
fn findings_verdict_upholding_one_finding_requeues() {
    let tmp = work_dir();
    let work = tmp.path();
    write_stage(work, &make_stage("s1"));
    let kind = DisputeKind::Findings {
        finding_ids: vec!["F-1-1".to_string(), "F-1-2".to_string()],
        evidence: Vec::<FindingSnapshot>::new(),
    };
    write_request(work, "s1", 1, kind);
    let rulings = vec![
        ruling("F-1-1", RulingKind::Dismiss),
        ruling("F-1-2", RulingKind::Uphold),
    ];
    write_verdict(work, "s1", 1, DisputeVerdict::Rulings { rulings }, 1);

    apply(work, "s1");

    let stage = load_stage("s1", work).unwrap();
    assert_eq!(stage.status, StageStatus::Queued);
    assert!(!is_held_for_completion(work, &stage));
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
        contracts: red_reports().iter().map(FrozenContract::from).collect(),
    };
    let copies = [(CONTRACT_FILE.to_string(), CONTRACT_CONTENT.to_vec())];
    contract_store::write_freeze(&work, &record, &copies).unwrap();
    (tmp, work, worktree)
}

#[test]
fn accepted_contract_verdict_requeues() {
    let (_tmp, work, worktree) = frozen_contract_fixture();
    let edited: &[u8] = b"#[test]\nfn rejects_x() { assert!(reject(\"x\")); }\n";
    std::fs::write(worktree.join(CONTRACT_FILE), edited).unwrap();
    let kind = DisputeKind::Contract {
        contract_id: CONTRACT_ID.to_string(),
    };
    write_request(&work, "s1", 1, kind);
    write_verdict(&work, "s1", 1, accept_without_patch(), 1);

    apply(&work, "s1");

    let stage = load_stage("s1", &work).unwrap();
    assert_eq!(stage.status, StageStatus::Queued);
    assert!(!is_held_for_completion(&work, &stage));
}
