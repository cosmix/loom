//! A contract dispute's replacement at admission: bounded, different from the
//! frozen file, bound to the frozen hash, and kept byte-exact in `request.md`.

use chrono::Utc;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

use super::*;
use crate::fs::work_dir::WorkDir;
use crate::models::dispute::{DisputeKind, MAX_REPLACEMENT_BYTES};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::adjudication::read_request;
use crate::utils::sha256_hex;
use crate::verify::contracts::store::{
    write_freeze, FreezeRecord, FrozenContract, FrozenFile, FREEZE_RECORD_VERSION,
};
use crate::verify::contracts::test_support::{
    contract, red_reports, CONTRACT_CONTENT, CONTRACT_FILE, CONTRACT_ID,
};
use crate::verify::transitions::save_stage;

const STAGE: &str = "stage-repl";

/// Bytes a text round trip would normalise: CRLF, trailing whitespace, a bare
/// `---` line (the request's own delimiter) and no final newline.
const TRICKY: &str = "line one\r\n---\ntrailing   \n\t---  \nend";

/// An executing plan v2 stage whose one contract is frozen over
/// [`CONTRACT_CONTENT`].
fn frozen_stage() -> (TempDir, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let workspace = WorkDir::new(tmp.path()).unwrap();
    workspace.initialize().unwrap();
    let work_dir = workspace.root().to_path_buf();
    let stage = Stage {
        id: STAGE.to_string(),
        name: "Repl".to_string(),
        status: StageStatus::Executing,
        plan_version: 2,
        contracts: vec![contract()],
        ..Stage::default()
    };
    save_stage(&stage, &work_dir).unwrap();
    let record = FreezeRecord {
        version: FREEZE_RECORD_VERSION,
        stage_id: STAGE.to_string(),
        session_id: "session-contract".to_string(),
        frozen_at: Utc::now(),
        base: "main".to_string(),
        files: vec![FrozenFile {
            path: CONTRACT_FILE.to_string(),
            sha256: sha256_hex(CONTRACT_CONTENT),
        }],
        contracts: red_reports()
            .iter()
            .map(|report| FrozenContract {
                file: Some(CONTRACT_FILE.to_string()),
                ..FrozenContract::from(report)
            })
            .collect(),
    };
    write_freeze(
        &work_dir,
        &record,
        &[(CONTRACT_FILE.to_string(), CONTRACT_CONTENT.to_vec())],
    )
    .unwrap();
    (tmp, work_dir)
}

fn dispute(replacement: Option<&str>, base: Option<&str>) -> DisputeKind {
    DisputeKind::Contract {
        contract_id: CONTRACT_ID.to_string(),
        replacement: replacement.map(str::to_string),
        replacement_base_sha256: base.map(str::to_string),
    }
}

fn file(work_dir: &Path, kind: DisputeKind) -> Response {
    handle_file_dispute(work_dir, STAGE, kind, "wrong".to_string(), None).unwrap()
}

fn disputes_spent(work_dir: &Path) -> u32 {
    load_stage(STAGE, work_dir).unwrap().tally.contract_disputes
}

fn request_exists(work_dir: &Path) -> bool {
    work_dir
        .join(format!("disputes/{STAGE}/1/request.md"))
        .exists()
}

fn refusal(response: Response) -> String {
    match response {
        Response::Error { message } => message,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn an_oversize_replacement_is_refused_without_spending_a_dispute() {
    let (_tmp, work_dir) = frozen_stage();
    let big = "x".repeat(MAX_REPLACEMENT_BYTES + 1);

    let message = refusal(file(&work_dir, dispute(Some(&big), None)));

    assert!(message.contains("over the 32768-byte limit"), "{message}");
    assert!(!request_exists(&work_dir));
    assert_eq!(disputes_spent(&work_dir), 0);
}

#[test]
fn a_replacement_identical_to_the_frozen_file_is_refused_without_spending_a_dispute() {
    let (_tmp, work_dir) = frozen_stage();
    let same = std::str::from_utf8(CONTRACT_CONTENT).unwrap();

    let message = refusal(file(&work_dir, dispute(Some(same), None)));

    assert!(message.contains("identical to the frozen"), "{message}");
    assert!(!request_exists(&work_dir));
    assert_eq!(disputes_spent(&work_dir), 0);
}

#[test]
fn request_md_keeps_the_replacement_byte_exact() {
    let (_tmp, work_dir) = frozen_stage();

    let Response::DisputeCreated { id } = file(&work_dir, dispute(Some(TRICKY), None)) else {
        panic!("the dispute was refused");
    };

    let request = read_request(&work_dir, STAGE, id).unwrap();
    let DisputeKind::Contract { replacement, .. } = request.kind else {
        panic!("not a contract dispute");
    };
    assert_eq!(replacement.as_deref(), Some(TRICKY));
    assert_eq!(disputes_spent(&work_dir), 1);
}

#[test]
fn admission_binds_the_replacement_to_the_frozen_hash_over_a_client_value() {
    let (_tmp, work_dir) = frozen_stage();

    let Response::DisputeCreated { id } = file(&work_dir, dispute(Some(TRICKY), Some("c0ffee")))
    else {
        panic!("the dispute was refused");
    };

    let request = read_request(&work_dir, STAGE, id).unwrap();
    let DisputeKind::Contract {
        replacement_base_sha256,
        ..
    } = request.kind
    else {
        panic!("not a contract dispute");
    };
    assert_eq!(replacement_base_sha256, Some(sha256_hex(CONTRACT_CONTENT)));
}

#[test]
fn a_dispute_without_a_replacement_keeps_no_base_hash() {
    let (_tmp, work_dir) = frozen_stage();

    let Response::DisputeCreated { id } = file(&work_dir, dispute(None, Some("c0ffee"))) else {
        panic!("the dispute was refused");
    };

    let request = read_request(&work_dir, STAGE, id).unwrap();
    assert_eq!(request.kind, dispute(None, None));
}
