//! Tests for the freeze handler, against a real git repository with the
//! stage worktree where loom puts it.

use super::*;
use crate::fs::session_files::save_session;
use crate::fs::work_dir::WorkDir;
use crate::models::session::{Session, SessionStatus};
use crate::verify::contracts::store::{frozen_file_path, load_freeze};
use crate::verify::contracts::test_support::{
    contract_stage, contract_worktree, liquidmon_contract, plant_foreign_git_dir, red_reports,
    write_liquidmon, CONTRACT_CONTENT, CONTRACT_FILE, LIQUIDMON_FILE,
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
    workspace.adopt_existing().unwrap();
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

/// Freeze from `reports`, as a host run that hashed the files as they stand
/// now would.
fn freeze(fx: &Fixture, reports: &[ContractRunReport]) -> Response {
    freeze_with_reports(
        &fx.work_dir,
        STAGE,
        &fx.session.id,
        reports,
        &hashed_now(fx),
    )
    .unwrap()
}

/// The `(path, sha256)` of every file the freeze reads now; empty when the
/// stage cannot be located (the freeze then refuses for that reason).
fn hashed_now(fx: &Fixture) -> Vec<(String, String)> {
    let stage = load_stage(STAGE, &fx.work_dir).unwrap();
    let Ok(site) = locate(&fx.work_dir, &stage) else {
        return Vec::new();
    };
    read_files(&site, &stage)
        .map(|files| {
            files
                .iter()
                .map(|(path, bytes)| (path.clone(), sha256_hex(bytes)))
                .collect()
        })
        .unwrap_or_default()
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
    assert_eq!(record.contracts[0].cwd.as_deref(), Some("."));
    let copy = frozen_file_path(&fx.work_dir, STAGE, CONTRACT_FILE);
    assert_eq!(std::fs::read(copy).unwrap(), CONTRACT_CONTENT);

    fx.session.session_type = SessionType::Stage;
    save_session(&fx.session, &fx.work_dir).unwrap();
    let message = refusal(freeze(&fx, &red_reports()));
    assert!(message.contains("contract session"), "{message}");
}

/// A contract whose file sits in a sub-workspace with its own runner config
/// runs from that sub-workspace, and the record says so.
#[test]
fn freeze_handler_records_the_run_dir_holding_the_runner_config() {
    let fx = fixture();
    std::fs::remove_file(fx.worktree.join(CONTRACT_FILE)).unwrap();
    let web = fx.worktree.join("apps/web");
    std::fs::create_dir_all(web.join("src")).unwrap();
    std::fs::write(web.join("bunfig.toml"), "").unwrap();
    std::fs::write(web.join("src/a.contract.test.ts"), "test('x', () => {});\n").unwrap();
    update_stage(STAGE, &fx.work_dir, |stage| {
        stage.contracts[0].file = "apps/web/src/a.contract.test.ts".to_string();
        stage.contracts[0].runner = Some("bun-test".to_string());
        stage.harness = vec!["apps/web/bunfig.toml".to_string()];
        Ok(())
    })
    .unwrap();
    let mut reports = red_reports();
    reports[0].adapter = Some("bun-test".to_string());

    let response = freeze(&fx, &reports);

    assert!(
        matches!(response, Response::ContractsFrozen { .. }),
        "{response:?}"
    );
    let record = load_freeze(&fx.work_dir, STAGE).unwrap().unwrap();
    assert_eq!(record.contracts[0].cwd.as_deref(), Some("apps/web"));
    let file = record.contracts[0].file.as_deref();
    assert_eq!(file, Some("apps/web/src/a.contract.test.ts"));
}

/// The writer of a stage waiting on its `AskUserQuestion` was answered and
/// froze before the monitor saw it working: the freeze is taken, and the
/// stage goes back to `Executing` for the handover.
#[test]
fn freeze_handler_takes_a_waiting_stage_back_to_executing() {
    let fx = fixture();
    update_stage(STAGE, &fx.work_dir, |stage| {
        stage.try_mark_waiting_for_input()?;
        stage.review_reason = Some("which runner should the contract use?".to_string());
        Ok(())
    })
    .unwrap();

    let response = freeze(&fx, &red_reports());

    assert!(
        matches!(response, Response::ContractsFrozen { .. }),
        "{response:?}"
    );
    let stage = load_stage(STAGE, &fx.work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::Executing);
    assert_eq!(stage.review_reason, None);
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

/// The scaffold edits a contract in a new module needs are not frozen: only
/// the contract file (and harness matches) are recorded.
#[test]
fn accepted_freeze_with_scaffold_records_only_contract_and_harness() {
    let fx = fixture();
    update_stage(STAGE, &fx.work_dir, |stage| {
        stage.contracts = vec![liquidmon_contract()];
        Ok(())
    })
    .unwrap();
    write_liquidmon(&fx.worktree);

    let response = freeze(&fx, &red_reports());

    assert!(
        matches!(response, Response::ContractsFrozen { files: 1 }),
        "{response:?}"
    );
    let record = load_freeze(&fx.work_dir, STAGE).unwrap().unwrap();
    let paths: Vec<&str> = record.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, [LIQUIDMON_FILE]);
}

/// Git lists no FIFO, so only the walk can see one planted where the
/// implementer will write; once it is gone the freeze is taken.
#[test]
fn freeze_handler_refuses_a_planted_fifo() {
    let fx = fixture();
    let fifo = fx.worktree.join("src/deep/lib.rs");
    std::fs::create_dir_all(fifo.parent().unwrap()).unwrap();
    nix::unistd::mkfifo(&fifo, nix::sys::stat::Mode::S_IRWXU).unwrap();

    let message = refusal(freeze(&fx, &red_reports()));

    assert!(message.contains("FIFO, socket or device node"), "{message}");
    assert!(message.contains("src/deep/lib.rs"), "{message}");
    assert!(load_freeze(&fx.work_dir, STAGE).unwrap().is_none());

    std::fs::remove_file(&fifo).unwrap();
    let response = freeze(&fx, &red_reports());
    assert!(
        matches!(response, Response::ContractsFrozen { files: 1 }),
        "{response:?}"
    );
}

/// The contract session repointed its `.git` file at a git directory whose
/// configuration defines a clean filter: the daemon's check reads the
/// stage's registered git directory, never runs the filter, and refuses the
/// changes it sees there.
#[test]
fn freeze_handler_ignores_a_repointed_git_file() {
    let fx = fixture();
    let marker = plant_foreign_git_dir(&fx.worktree);

    let message = refusal(freeze(&fx, &red_reports()));

    assert!(!marker.exists(), "the daemon ran the stage's clean filter");
    assert!(message.contains("README.md"), "{message}");
    assert!(message.contains(".gitattributes"), "{message}");
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
