//! Tests for the freeze handler, against a real git repository with the
//! stage worktree where loom puts it.

use super::*;
use crate::fs::session_files::save_session;
use crate::fs::work_dir::WorkDir;
use crate::models::session::{Session, SessionStatus};
use crate::verify::contracts::store::{frozen_file_path, load_freeze};
use crate::verify::contracts::test_support::{
    contract_stage, contract_worktree, red_reports, CONTRACT_CONTENT, CONTRACT_FILE,
};
use crate::verify::transitions::{save_stage, update_stage};
use tempfile::TempDir;

const STAGE: &str = "s1";

struct Fixture {
    _tmp: TempDir,
    work_dir: PathBuf,
    worktree: PathBuf,
    session: Session,
}

/// The stage worktree holding the contract file, and the stage's running
/// contract session.
fn fixture() -> Fixture {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    let worktree = contract_worktree(&repo, STAGE);
    let workspace = WorkDir::new(&repo).unwrap();
    workspace.initialize().unwrap();
    let work_dir = workspace.root().to_path_buf();

    let mut session = Session::new();
    session.session_type = SessionType::Contract;
    session.assign_to_stage(STAGE.to_string());
    session.status = SessionStatus::Running;
    save_session(&session, &work_dir).unwrap();
    save_stage(&contract_stage(STAGE, &session.id), &work_dir).unwrap();
    Fixture {
        _tmp: tmp,
        work_dir,
        worktree,
        session,
    }
}

fn freeze(fx: &Fixture, reports: &[ContractRunReport]) -> Response {
    handle_freeze_contracts(&fx.work_dir, STAGE, &fx.session.id, reports).unwrap()
}

fn refusal(response: Response) -> String {
    match response {
        Response::Error { message } => message,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn freeze_handler_records_hashes_and_copies() {
    let mut fx = fixture();

    let response = freeze(&fx, &red_reports());

    assert!(
        matches!(response, Response::ContractsFrozen { files: 1 }),
        "{response:?}"
    );
    let record = load_freeze(&fx.work_dir, STAGE).unwrap().unwrap();
    assert_eq!(record.session_id, fx.session.id);
    assert_eq!(record.files.len(), 1);
    assert_eq!(record.files[0].path, CONTRACT_FILE);
    assert_eq!(record.files[0].sha256, sha256_hex(CONTRACT_CONTENT));
    assert_eq!(record.contracts[0].outcome, "failed");
    let copy = frozen_file_path(&fx.work_dir, STAGE, CONTRACT_FILE);
    assert_eq!(std::fs::read(copy).unwrap(), CONTRACT_CONTENT);

    fx.session.session_type = SessionType::Stage;
    save_session(&fx.session, &fx.work_dir).unwrap();
    let message = refusal(freeze(&fx, &red_reports()));
    assert!(message.contains("contract session"), "{message}");
}

#[test]
fn freeze_handler_refuses_changes_outside_contracts_and_harness() {
    let fx = fixture();
    std::fs::create_dir_all(fx.worktree.join("src")).unwrap();
    std::fs::write(fx.worktree.join("src/lib.rs"), "pub fn x() {}\n").unwrap();

    let message = refusal(freeze(&fx, &red_reports()));

    assert!(message.contains("src/lib.rs"), "{message}");
    assert!(load_freeze(&fx.work_dir, STAGE).unwrap().is_none());
}

#[test]
fn freeze_handler_refuses_a_passing_or_missing_report() {
    let fx = fixture();
    let mut passing = red_reports();
    passing[0].outcome = "passed".to_string();

    assert!(refusal(freeze(&fx, &passing)).contains("only while it fails"));
    assert!(refusal(freeze(&fx, &[])).contains("exactly one run report"));
    assert!(load_freeze(&fx.work_dir, STAGE).unwrap().is_none());
}

#[test]
fn freeze_handler_refuses_a_stage_that_is_not_standard() {
    let fx = fixture();
    update_stage(STAGE, &fx.work_dir, |stage| {
        stage.stage_type = StageType::IntegrationVerify;
        Ok(())
    })
    .unwrap();

    let message = refusal(freeze(&fx, &red_reports()));

    assert!(message.contains("no contracts to freeze"), "{message}");
    assert!(load_freeze(&fx.work_dir, STAGE).unwrap().is_none());
}
