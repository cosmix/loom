//! The fixture of `relay_dispute_replacement_e2e.rs`: a repository with a
//! registered worktree at `.worktrees/<stage>`, a Running Stage session, and a
//! plan v2 stage with one frozen contract. The fixture follows
//! `relay_commit_e2e.rs`.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use chrono::Utc;
use loom::fs::session_files::save_session;
use loom::models::session::{Session, SessionStatus, SessionType};
use loom::models::stage::{Stage, StageStatus};
use loom::orchestrator::{Orchestrator, OrchestratorConfig};
use loom::plan::schema::{AcceptanceCriterion, ContractSpec};
use loom::verify::contracts::store::{write_freeze, FreezeRecord, FrozenContract, FrozenFile};
use loom::verify::transitions::save_stage;
use tempfile::TempDir;
use uuid::Uuid;

use super::super::helpers;
use super::super::helpers::test_support::git_repo::{self, git, TestRepo};
use super::super::relay_scratch::{create_scratch_dir, tempdir_outside_tmp};

pub(super) const STAGE: &str = "relay-replacement-stage";
pub(super) const CONTRACT_ID: &str = "rejects-x";
pub(super) const CONTRACT_FILE: &str = "tests/x_contract.rs";
pub(super) const FROZEN: &str = "#[test]\nfn rejects_x() { assert!(false); }\n";
pub(super) const REASON: &str = "r";

/// Everything one run needs; the temp dirs live as long as the fixture.
pub(super) struct Fixture {
    _dirs: Vec<TempDir>,
    pub(super) repo_root: PathBuf,
    pub(super) work_dir: PathBuf,
    pub(super) worktree_path: PathBuf,
    pub(super) home: PathBuf,
    pub(super) xdg: PathBuf,
    pub(super) scratch_dir: PathBuf,
    pub(super) session_id: String,
}

pub(super) fn sha256_hex(text: &str) -> String {
    loom::utils::sha256_hex(text.as_bytes())
}

pub(super) fn contract_spec() -> ContractSpec {
    ContractSpec {
        id: CONTRACT_ID.to_string(),
        file: CONTRACT_FILE.to_string(),
        test: Some("tests::rejects_x".to_string()),
        runner: Some("cargo-test".to_string()),
        cwd: None,
        scenario: "x arrives".to_string(),
        rejects: "accepting x".to_string(),
    }
}

/// A Running Stage session and an Executing plan v2 stage it owns, with the pid
/// entry of the test process itself so a hook child proves as "inside the
/// session".
pub(super) fn write_session_and_stage(work_dir: &Path, worktree: &Path, session_id: &str) {
    let mut session = Session::new();
    session.id = session_id.to_string();
    session.session_type = SessionType::Stage;
    session.stage_id = Some(STAGE.to_string());
    session.tracking_key = Session::derive_tracking_key(STAGE, SessionType::Stage);
    session.status = SessionStatus::Running;
    session.worktree_path = Some(worktree.to_path_buf());
    save_session(&session, work_dir).expect("save session record");

    let pid = std::process::id();
    let start_time = loom::process::process_start_time(pid).expect("own process start time");
    let pids_dir = work_dir.join("pids");
    fs::create_dir_all(&pids_dir).expect("create pids dir");
    let pid_file = format!("{}-{session_id}.pid", session.tracking_key);
    fs::write(pids_dir.join(pid_file), format!("{pid}\n{start_time}\n")).expect("write pid entry");

    let stage = Stage {
        id: STAGE.to_string(),
        name: STAGE.to_string(),
        status: StageStatus::Executing,
        plan_version: 2,
        session: Some(session_id.to_string()),
        worktree: Some(STAGE.to_string()),
        contracts: vec![contract_spec()],
        acceptance: vec![AcceptanceCriterion::Simple("true".to_string())],
        ..Default::default()
    };
    save_stage(&stage, work_dir).expect("save stage record");
}

/// The stage's contract, frozen over [`FROZEN`] and present in the worktree.
pub(super) fn freeze_contract(work_dir: &Path, worktree: &Path) {
    let file = worktree.join(CONTRACT_FILE);
    fs::create_dir_all(file.parent().unwrap()).expect("create tests dir");
    fs::write(&file, FROZEN).expect("write contract file");
    let record = FreezeRecord {
        version: loom::verify::contracts::store::FREEZE_RECORD_VERSION,
        stage_id: STAGE.to_string(),
        session_id: "contract-session".to_string(),
        frozen_at: Utc::now(),
        base: "main".to_string(),
        files: vec![FrozenFile {
            path: CONTRACT_FILE.to_string(),
            sha256: sha256_hex(FROZEN),
        }],
        contracts: vec![FrozenContract {
            id: CONTRACT_ID.to_string(),
            adapter: Some("cargo-test".to_string()),
            outcome: "red".to_string(),
            exit_code: Some(101),
            file: Some(CONTRACT_FILE.to_string()),
            cwd: Some(".".to_string()),
            test: Some("tests::rejects_x".to_string()),
        }],
    };
    let copies = [(CONTRACT_FILE.to_string(), FROZEN.as_bytes().to_vec())];
    write_freeze(work_dir, &record, &copies).expect("write the freeze record");
}

pub(super) fn build_fixture() -> Fixture {
    let home = tempdir_outside_tmp("home-");
    let xdg = tempdir_outside_tmp("xdg-");
    let repo = TempDir::new().expect("create repo dir");
    let repo_root = repo.path().to_path_buf();
    TestRepo::builder().no_seed().init_in(&repo_root);
    git(&repo_root, &["commit", "--allow-empty", "-m", "initial"]);
    let worktree_path = repo_root.join(".worktrees").join(STAGE);
    let branch = format!("loom/{STAGE}");
    git(
        &repo_root,
        &[
            "worktree",
            "add",
            "-b",
            &branch,
            worktree_path.to_str().unwrap(),
        ],
    );
    let work_dir = repo_root.join(".loom").join("work");
    fs::create_dir_all(&work_dir).expect("create work dir");
    helpers::write_main_checkout_identity(&repo_root, &work_dir);
    // A stage worktree reaches the shared state through this symlink.
    fs::create_dir_all(worktree_path.join(".loom")).expect("create worktree .loom dir");
    std::os::unix::fs::symlink(&work_dir, worktree_path.join(".loom/work"))
        .expect("link worktree .loom/work to the shared state");

    let session_id = format!(
        "relay-replacement-{}-{}",
        std::process::id(),
        Uuid::new_v4().simple()
    );
    write_session_and_stage(&work_dir, &worktree_path, &session_id);
    freeze_contract(&work_dir, &worktree_path);
    let scratch_dir = create_scratch_dir(xdg.path(), home.path(), &session_id);

    let (home_path, xdg_path) = (home.path().to_path_buf(), xdg.path().to_path_buf());
    Fixture {
        _dirs: vec![repo, home, xdg],
        repo_root,
        work_dir,
        worktree_path,
        home: home_path,
        xdg: xdg_path,
        scratch_dir,
        session_id,
    }
}

impl Fixture {
    /// A `loom` command (via `helpers::loom_cmd`) as the Stage session.
    pub(super) fn session_loom(&self) -> Command {
        let mut command = helpers::loom_cmd();
        command
            .env_remove("LOOM_STAGE_ID")
            .env_remove("LOOM_SESSION_ID")
            .current_dir(&self.worktree_path)
            .env("LOOM_SESSION_ID", &self.session_id)
            .env("LOOM_STAGE_ID", STAGE)
            .env("LOOM_SESSION_TYPE", "stage")
            .env("LOOM_SCRATCH_DIR", &self.scratch_dir)
            .env("LOOM_WORK_DIR", &self.work_dir)
            .env("LOOM_WORKTREE_PATH", &self.worktree_path)
            .env("XDG_RUNTIME_DIR", &self.xdg)
            .env("HOME", &self.home);
        git_repo::isolate(&mut command);
        command
    }

    /// Write `text` to a replacement file in the scratch directory.
    pub(super) fn write_replacement(&self, text: &str) -> PathBuf {
        let path = self.scratch_dir.join("replacement.rs");
        fs::write(&path, text).expect("write replacement");
        path
    }

    /// The `loom stage dispute-contract` command line for `replacement`.
    pub(super) fn dispute_command(&self, replacement: &Path) -> String {
        format!(
            "loom stage dispute-contract {STAGE} --contract {CONTRACT_ID} --replacement {} \
             --reason {REASON}",
            replacement.display()
        )
    }

    pub(super) fn run_dispute(&self, replacement: &Path) -> Output {
        let mut command = self.session_loom();
        command
            .args([
                "stage",
                "dispute-contract",
                STAGE,
                "--contract",
                CONTRACT_ID,
            ])
            .arg("--replacement")
            .arg(replacement)
            .args(["--reason", REASON]);
        command
            .stdin(Stdio::null())
            .output()
            .expect("spawn loom stage dispute-contract")
    }

    pub(super) fn tickets(&self) -> Vec<PathBuf> {
        let entries = fs::read_dir(&self.scratch_dir).expect("read scratch dir");
        let mut found: Vec<PathBuf> = entries
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| path.extension().is_some_and(|ext| ext == "req"))
            .collect();
        found.sort();
        found
    }
}

/// Run the real `loom-relay.sh` as a direct child of THIS process, so its
/// `loom hook relay` grandchild sits inside the recorded session process tree.
pub(super) fn run_hook(fx: &Fixture, command_text: &str, filed: &Output) -> Output {
    let payload = serde_json::json!({
        "tool_name": "Bash",
        "tool_input": {"command": command_text},
        "tool_response": {
            "stdout": String::from_utf8_lossy(&filed.stdout),
            "stderr": String::from_utf8_lossy(&filed.stderr),
        },
    })
    .to_string();
    let hook_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../loom-hooks/loom-relay.sh");
    let real_path = std::env::var_os("PATH").unwrap_or_default();

    let mut command = Command::new("bash");
    command
        .arg(&hook_path)
        .env_remove("LOOM_STAGE_ID")
        .env_remove("LOOM_SESSION_ID")
        .env("LOOM_SESSION_ID", &fx.session_id)
        .env("LOOM_STAGE_ID", STAGE)
        .env("LOOM_SCRATCH_DIR", &fx.scratch_dir)
        .env("LOOM_WORK_DIR", &fx.work_dir)
        .env("XDG_RUNTIME_DIR", &fx.xdg)
        .env("HOME", &fx.home)
        .env("LOOM_BIN", helpers::loom_bin_path())
        .env("LOOM_HOOK_PATH", &real_path)
        .env("PATH", &real_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    git_repo::isolate(&mut command);
    let mut child = command.spawn().expect("spawn loom-relay.sh");
    let stdin = child.stdin.take().expect("child stdin is piped");
    { stdin }
        .write_all(payload.as_bytes())
        .expect("write hook payload");
    child.wait_with_output().expect("wait for loom-relay.sh")
}

pub(super) fn drain_once(fx: &Fixture) {
    let graph = helpers::build_test_graph(vec![(STAGE, vec![])]);
    let config = OrchestratorConfig {
        work_dir: fx.work_dir.clone(),
        repo_root: fx.repo_root.clone(),
        manual_mode: true,
        enable_skill_routing: false,
        ..Default::default()
    };
    let mut orchestrator = Orchestrator::new(config, graph).expect("construct orchestrator");
    orchestrator.drain_session_inboxes();
}
