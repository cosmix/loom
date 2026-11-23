//! A contract dispute at admission: distinct frozen contracts, replacements
//! keyed by frozen file, bounded, different from the frozen file, bound to the
//! frozen hash, and kept byte-exact in `request.md`; no filing while a frozen
//! file has drifted in the worktree.

use chrono::Utc;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

use super::*;
use crate::fs::work_dir::WorkDir;
use crate::models::dispute::{DisputeKind, FileReplacement, MAX_REPLACEMENT_BYTES};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::adjudication::read_request;
use crate::plan::schema::ContractSpec;
use crate::stage_service::ContractRunReport;
use crate::utils::sha256_hex;
use crate::verify::contracts::store::{
    write_freeze, FreezeRecord, FrozenContract, FrozenFile, FREEZE_RECORD_VERSION,
};
use crate::verify::contracts::test_support::{
    contract, contract_worktree, CONTRACT_CONTENT, CONTRACT_FILE, CONTRACT_ID,
};
use crate::verify::transitions::save_stage;

#[path = "dispute_trial_tests.rs"]
mod trial_tests;
use trial_tests::file_trialed;

const STAGE: &str = "stage-repl";
const OTHER_ID: &str = "rejects-y";
const OTHER_FILE: &str = "tests/y_contract.rs";
const OTHER_CONTENT: &[u8] = b"#[test]\nfn rejects_y() { assert!(false); }\n";

/// Bytes a text round trip would normalise: CRLF, trailing whitespace, a bare
/// `---` line (the request's own delimiter) and no final newline.
const TRICKY: &str = "line one\r\n---\ntrailing   \n\t---  \nend";

fn other_contract(file: &str) -> ContractSpec {
    ContractSpec {
        id: OTHER_ID.to_string(),
        file: file.to_string(),
        test: Some("rejects_y".to_string()),
        ..contract()
    }
}

/// An executing plan v2 stage whose one contract is frozen over
/// [`CONTRACT_CONTENT`], its worktree holding the frozen file.
fn frozen_stage() -> (TempDir, PathBuf) {
    frozen_stage_with(vec![contract()], &[(CONTRACT_FILE, CONTRACT_CONTENT)])
}

/// An executing plan v2 stage with `contracts`, all frozen over `files`, and
/// its worktree `.worktrees/<stage>` holding each frozen file.
fn frozen_stage_with(contracts: Vec<ContractSpec>, files: &[(&str, &[u8])]) -> (TempDir, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let workspace = WorkDir::new(tmp.path()).unwrap();
    workspace.initialize().unwrap();
    let work_dir = workspace.root().to_path_buf();
    // A registered worktree on a committed base: admission reads its HEAD to
    // match a replacement trial.
    let worktree = contract_worktree(tmp.path(), STAGE);
    for (path, content) in files {
        let path = worktree.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }
    let record = FreezeRecord {
        version: FREEZE_RECORD_VERSION,
        stage_id: STAGE.to_string(),
        session_id: "session-contract".to_string(),
        frozen_at: Utc::now(),
        base: "main".to_string(),
        files: files
            .iter()
            .map(|(path, content)| FrozenFile {
                path: path.to_string(),
                sha256: sha256_hex(content),
            })
            .collect(),
        contracts: contracts.iter().map(frozen_contract).collect(),
    };
    let stage = Stage {
        id: STAGE.to_string(),
        name: "Repl".to_string(),
        status: StageStatus::Executing,
        plan_version: 2,
        contracts,
        ..Stage::default()
    };
    save_stage(&stage, &work_dir).unwrap();
    let frozen: Vec<(String, Vec<u8>)> = files
        .iter()
        .map(|(path, content)| (path.to_string(), content.to_vec()))
        .collect();
    write_freeze(&work_dir, &record, &frozen).unwrap();
    (tmp, work_dir)
}

fn frozen_contract(spec: &ContractSpec) -> FrozenContract {
    let report = ContractRunReport {
        contract_id: spec.id.clone(),
        adapter: Some("cargo-test".to_string()),
        outcome: "failed".to_string(),
        exit_code: Some(101),
    };
    FrozenContract {
        file: Some(spec.file.clone()),
        ..FrozenContract::from(&report)
    }
}

fn worktree_file(work_dir: &Path, path: &str) -> PathBuf {
    let repo_root = work_dir.parent().unwrap().parent().unwrap();
    repo_root.join(".worktrees").join(STAGE).join(path)
}

fn replacement(file: &str, content: &str, base: Option<&str>) -> FileReplacement {
    FileReplacement {
        file: file.to_string(),
        content: content.to_string(),
        base_sha256: base.map(str::to_string),
    }
}

fn contracts(ids: &[&str], replacements: Vec<FileReplacement>) -> DisputeKind {
    DisputeKind::Contract {
        contract_ids: ids.iter().map(|id| id.to_string()).collect(),
        replacements,
    }
}

fn dispute(content: Option<&str>, base: Option<&str>) -> DisputeKind {
    let replacements = content
        .map(|content| vec![replacement(CONTRACT_FILE, content, base)])
        .unwrap_or_default();
    contracts(&[CONTRACT_ID], replacements)
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

/// `kind` is refused with a message containing each of `needles`, and no
/// request is written nor dispute spent.
fn assert_refused(work_dir: &Path, kind: DisputeKind, needles: &[&str]) {
    let message = refusal(file(work_dir, kind));
    for needle in needles {
        assert!(message.contains(needle), "{needle:?} not in: {message}");
    }
    assert!(!request_exists(work_dir));
    assert_eq!(disputes_spent(work_dir), 0);
}

#[test]
fn an_oversize_replacement_is_refused_without_spending_a_dispute() {
    let (_tmp, work_dir) = frozen_stage();
    let big = "x".repeat(MAX_REPLACEMENT_BYTES + 1);
    assert_refused(
        &work_dir,
        dispute(Some(&big), None),
        &["over the 32768-byte limit"],
    );
}

#[test]
fn a_replacement_identical_to_the_frozen_file_is_refused_without_spending_a_dispute() {
    let (_tmp, work_dir) = frozen_stage();
    let same = std::str::from_utf8(CONTRACT_CONTENT).unwrap();
    assert_refused(
        &work_dir,
        dispute(Some(same), None),
        &["identical to the frozen"],
    );
}

#[test]
fn a_filing_naming_no_contract_or_one_twice_is_refused() {
    let (_tmp, work_dir) = frozen_stage();
    assert_refused(
        &work_dir,
        contracts(&[], Vec::new()),
        &["no contract is named"],
    );
    assert_refused(
        &work_dir,
        contracts(&[CONTRACT_ID, CONTRACT_ID], Vec::new()),
        &[CONTRACT_ID, "named twice"],
    );
}

#[test]
fn a_replacement_for_a_file_the_freeze_does_not_hold_is_refused() {
    let (_tmp, work_dir) = frozen_stage();
    let kind = contracts(
        &[CONTRACT_ID],
        vec![replacement("tests/unfrozen.rs", TRICKY, None)],
    );
    assert_refused(&work_dir, kind, &["'tests/unfrozen.rs' is no frozen file"]);
}

#[test]
fn a_replacement_changing_an_undisputed_contract_in_its_file_is_refused() {
    // Both contracts live in one frozen file; replacing it changes both.
    let (_tmp, work_dir) = frozen_stage_with(
        vec![contract(), other_contract(CONTRACT_FILE)],
        &[(CONTRACT_FILE, CONTRACT_CONTENT)],
    );
    let kind = contracts(
        &[CONTRACT_ID],
        vec![replacement(CONTRACT_FILE, TRICKY, None)],
    );
    assert_refused(
        &work_dir,
        kind,
        &["does not dispute", "--contract rejects-y"],
    );
}

#[test]
fn a_contract_an_amendment_deleted_neither_holds_its_file_nor_can_be_disputed() {
    // Both were frozen in one file; an accepted amendment then deleted
    // `rejects-y` from the stage, while the record still lists it.
    let (_tmp, work_dir) = frozen_stage_with(
        vec![contract(), other_contract(CONTRACT_FILE)],
        &[(CONTRACT_FILE, CONTRACT_CONTENT)],
    );
    crate::verify::transitions::update_stage(STAGE, &work_dir, |stage| {
        stage.contracts.retain(|spec| spec.id != OTHER_ID);
        Ok(())
    })
    .unwrap();

    assert_refused(
        &work_dir,
        contracts(&[OTHER_ID], Vec::new()),
        &["'rejects-y' names no frozen contract"],
    );
    let kind = contracts(
        &[CONTRACT_ID],
        vec![replacement(CONTRACT_FILE, TRICKY, None)],
    );
    assert!(matches!(
        file_trialed(&work_dir, kind),
        Response::DisputeCreated { .. }
    ));
}

#[test]
fn a_filing_while_a_frozen_file_drifted_in_the_worktree_is_refused() {
    let (_tmp, work_dir) = frozen_stage();
    std::fs::write(worktree_file(&work_dir, CONTRACT_FILE), TRICKY).unwrap();
    assert_refused(
        &work_dir,
        dispute(None, None),
        &[
            "frozen file 'tests/x_contract.rs' changed in the worktree",
            "loom stage contracts restore stage-repl",
            "--replacement tests/x_contract.rs=<file>",
        ],
    );
}

#[test]
fn a_two_contract_two_file_filing_spends_one_dispute() {
    let (_tmp, work_dir) = frozen_stage_with(
        vec![contract(), other_contract(OTHER_FILE)],
        &[
            (CONTRACT_FILE, CONTRACT_CONTENT),
            (OTHER_FILE, OTHER_CONTENT),
        ],
    );
    let kind = contracts(
        &[CONTRACT_ID, OTHER_ID],
        vec![
            replacement(CONTRACT_FILE, TRICKY, None),
            replacement(OTHER_FILE, "fn other() {}\n", None),
        ],
    );

    let Response::DisputeCreated { id } = file_trialed(&work_dir, kind) else {
        panic!("the dispute was refused");
    };

    assert_eq!(disputes_spent(&work_dir), 1);
    let request = read_request(&work_dir, STAGE, id).unwrap();
    let DisputeKind::Contract {
        contract_ids,
        replacements,
    } = request.kind
    else {
        panic!("not a contract dispute");
    };
    assert_eq!(contract_ids, [CONTRACT_ID, OTHER_ID]);
    let bases: Vec<Option<String>> = replacements.into_iter().map(|r| r.base_sha256).collect();
    assert_eq!(
        bases,
        [
            Some(sha256_hex(CONTRACT_CONTENT)),
            Some(sha256_hex(OTHER_CONTENT))
        ]
    );
}

#[test]
fn request_md_keeps_the_replacement_byte_exact() {
    let (_tmp, work_dir) = frozen_stage();

    let Response::DisputeCreated { id } = file_trialed(&work_dir, dispute(Some(TRICKY), None))
    else {
        panic!("the dispute was refused");
    };

    let request = read_request(&work_dir, STAGE, id).unwrap();
    let DisputeKind::Contract { replacements, .. } = request.kind else {
        panic!("not a contract dispute");
    };
    assert_eq!(replacements[0].content, TRICKY);
    assert_eq!(disputes_spent(&work_dir), 1);
}

#[test]
fn admission_binds_the_replacement_to_the_frozen_hash_over_a_client_value() {
    let (_tmp, work_dir) = frozen_stage();

    let Response::DisputeCreated { id } =
        file_trialed(&work_dir, dispute(Some(TRICKY), Some("c0ffee")))
    else {
        panic!("the dispute was refused");
    };

    let request = read_request(&work_dir, STAGE, id).unwrap();
    let DisputeKind::Contract { replacements, .. } = request.kind else {
        panic!("not a contract dispute");
    };
    assert_eq!(
        replacements[0].base_sha256,
        Some(sha256_hex(CONTRACT_CONTENT))
    );
}

#[test]
fn a_dispute_without_a_replacement_records_none() {
    let (_tmp, work_dir) = frozen_stage();

    let Response::DisputeCreated { id } = file(&work_dir, dispute(None, None)) else {
        panic!("the dispute was refused");
    };

    let request = read_request(&work_dir, STAGE, id).unwrap();
    assert_eq!(request.kind, dispute(None, None));
}
