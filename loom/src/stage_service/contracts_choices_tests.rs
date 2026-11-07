//! The freeze handler and the writer's choices: admission validates them and
//! saves only what passes, a stale saved choice is pruned, the freeze records
//! the chosen test, and a request that would join a running freeze under
//! other choices is refused with the saved file untouched.

use std::path::PathBuf;

use chrono::Utc;
use serial_test::serial;
use tempfile::TempDir;

use super::super::{freeze_with_reports, handle_freeze_contracts, locate, read_files};
use crate::fs::session_files::save_session;
use crate::fs::work_dir::WorkDir;
use crate::host_verify::{digest, new_job_id, store as jobs, JobKind, JobRecord, JobState};
use crate::models::session::{Session, SessionStatus, SessionType};
use crate::plan::schema::{ContractChoice, ContractChoices};
use crate::relay::sha256_hex;
use crate::stage_service::Response;
use crate::verify::contracts::choices::{load_choices, load_effective_stage, save_choices};
use crate::verify::contracts::store::load_freeze;
use crate::verify::contracts::test_support::{
    contract_stage, contract_worktree, red_reports, CONTRACT_ID,
};
use crate::verify::transitions::save_stage;

const STAGE: &str = "s1";
const CHOSEN: &str = "tests::rejects_x";

/// The cargo stage worktree of `test_support`, whose one contract has no
/// test hint, under its running contract session.
struct Fixture {
    _tmp: TempDir,
    work_dir: PathBuf,
    worktree: PathBuf,
    session_id: String,
}

fn fixture() -> Fixture {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().canonicalize().unwrap().join("repo");
    let worktree = contract_worktree(&repo, STAGE);
    let workspace = WorkDir::new(&repo).unwrap();
    workspace.adopt_existing().unwrap();
    let work_dir = workspace.root().to_path_buf();
    let mut session = Session::new();
    session.session_type = SessionType::Contract;
    session.assign_to_stage(STAGE.to_string());
    session.status = SessionStatus::Running;
    save_session(&session, &work_dir).unwrap();
    let mut stage = contract_stage(STAGE, &session.id);
    stage.contracts[0].test = None;
    save_stage(&stage, &work_dir).unwrap();
    Fixture {
        _tmp: tmp,
        work_dir,
        worktree,
        session_id: session.id,
    }
}

impl Fixture {
    fn head(&self) -> String {
        digest::head(&self.worktree).unwrap()
    }

    fn tree(&self) -> String {
        digest::tree_digest(&self.worktree).unwrap()
    }

    /// The writer's freeze request for `tree` with `choices`.
    fn request(&self, tree: &str, choices: &ContractChoices) -> Response {
        let (stage, session) = (STAGE, self.session_id.as_str());
        let head = self.head();
        handle_freeze_contracts(&self.work_dir, stage, session, &head, tree, "r1", choices).unwrap()
    }

    fn choices_file(&self) -> PathBuf {
        self.work_dir
            .join("contracts")
            .join(STAGE)
            .join("choices.json")
    }

    /// A queued freeze job for the worktree's head and tree.
    fn pending_freeze(&self) -> JobRecord {
        // The order counter lives in the stage directory the lock creates.
        let _lock = jobs::lock_stage(&self.work_dir, STAGE).unwrap();
        let record = JobRecord {
            id: new_job_id(),
            submission_order: jobs::next_submission_order(&self.work_dir, STAGE).unwrap(),
            stage_id: STAGE.to_string(),
            session_id: self.session_id.clone(),
            kind: JobKind::Freeze,
            head: self.head(),
            tree_digest: self.tree(),
            no_cache: false,
            criteria: None,
            state: JobState::Queued,
            requests: vec!["r0".to_string()],
            queued_at: Utc::now(),
            started_at: None,
            finished_at: None,
            pid: None,
            detail: None,
        };
        jobs::save_current(&self.work_dir, &record).unwrap();
        record
    }
}

fn test_choice(test: &str) -> ContractChoices {
    let choice = ContractChoice {
        test: Some(test.to_string()),
        ..ContractChoice::default()
    };
    ContractChoices::from([(CONTRACT_ID.to_string(), choice)])
}

#[test]
#[serial]
fn refused_choices_are_answered_and_saved_nowhere() {
    let fx = fixture();
    let mut stray = test_choice(CHOSEN);
    stray.insert("c9".to_string(), ContractChoice::default());
    let response = fx.request(&fx.tree(), &stray);
    assert!(matches!(response, Response::Error { .. }), "{response:?}");
    let missing = fx.request(&fx.tree(), &ContractChoices::new());
    let Response::Error { message } = missing else {
        panic!("a contract without a test is refused, got {missing:?}");
    };
    assert!(
        message.contains("--test rejects-x=<identifier>"),
        "{message}"
    );
    assert!(!fx.choices_file().exists());
    assert!(jobs::load_current(&fx.work_dir, STAGE).unwrap().is_none());
}

#[test]
#[serial]
fn admitted_choices_are_saved_and_the_freeze_records_the_test() {
    let fx = fixture();
    assert_eq!(fx.request(&fx.tree(), &test_choice(CHOSEN)), Response::Ok);
    assert_eq!(
        load_choices(&fx.work_dir, STAGE).unwrap(),
        test_choice(CHOSEN)
    );

    let stage = load_effective_stage(STAGE, &fx.work_dir).unwrap();
    let site = locate(&fx.work_dir, &stage).unwrap();
    let verified: Vec<(String, String)> = read_files(&site, &stage)
        .unwrap()
        .iter()
        .map(|(path, bytes)| (path.clone(), sha256_hex(bytes)))
        .collect();
    let session = &fx.session_id;
    let frozen = freeze_with_reports(&fx.work_dir, STAGE, session, &red_reports(), &verified);
    assert!(
        matches!(frozen.unwrap(), Response::ContractsFrozen { .. }),
        "the chosen test lets the open contract freeze"
    );
    let record = load_freeze(&fx.work_dir, STAGE).unwrap().unwrap();
    assert_eq!(record.contracts[0].test.as_deref(), Some(CHOSEN));
}

#[test]
#[serial]
fn a_stale_saved_choice_refuses_nothing_and_is_pruned() {
    let fx = fixture();
    let mut saved = test_choice(CHOSEN);
    let stale = ContractChoice {
        runner: Some("vitest".to_string()),
        ..ContractChoice::default()
    };
    saved.insert("removed-by-amendment".to_string(), stale);
    save_choices(&fx.work_dir, STAGE, &saved).unwrap();
    assert_eq!(
        fx.request(&fx.tree(), &ContractChoices::new()),
        Response::Ok
    );
    assert_eq!(
        load_choices(&fx.work_dir, STAGE).unwrap(),
        test_choice(CHOSEN)
    );
}

#[test]
#[serial]
fn changed_choices_do_not_join_a_running_freeze_of_the_same_tree() {
    let fx = fixture();
    save_choices(&fx.work_dir, STAGE, &test_choice(CHOSEN)).unwrap();
    let before = std::fs::read(fx.choices_file()).unwrap();
    let pending = fx.pending_freeze();

    let changed = fx.request(&fx.tree(), &test_choice("tests::other"));
    let Response::Error { message } = changed else {
        panic!("changed flags are refused, got {changed:?}");
    };
    assert!(
        message.contains("in progress under other flags"),
        "{message}"
    );
    assert_eq!(std::fs::read(fx.choices_file()).unwrap(), before);

    assert_eq!(fx.request(&fx.tree(), &test_choice(CHOSEN)), Response::Ok);
    let joined = jobs::load_current(&fx.work_dir, STAGE).unwrap().unwrap();
    assert_eq!(joined.id, pending.id, "equal choices join the running job");
    assert_eq!(std::fs::read(fx.choices_file()).unwrap(), before);

    std::fs::write(fx.worktree.join("notes.txt"), "another tree\n").unwrap();
    let other_tree = fx.tree();
    assert_ne!(other_tree, pending.tree_digest);
    assert_eq!(
        fx.request(&other_tree, &test_choice("tests::other")),
        Response::Ok
    );
    assert_eq!(
        load_choices(&fx.work_dir, STAGE).unwrap(),
        test_choice("tests::other")
    );
    let current = jobs::load_current(&fx.work_dir, STAGE).unwrap().unwrap();
    assert_ne!(
        current.id, pending.id,
        "the request for another tree supersedes"
    );
}

#[test]
#[serial]
fn a_value_is_saved_trimmed() {
    let fx = fixture();
    let untrimmed = format!("  {CHOSEN}\t ");
    assert_eq!(
        fx.request(&fx.tree(), &test_choice(&untrimmed)),
        Response::Ok
    );
    assert_eq!(
        load_choices(&fx.work_dir, STAGE).unwrap(),
        test_choice(CHOSEN)
    );
}
