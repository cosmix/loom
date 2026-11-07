//! Fixtures for the criteria-authoring contracts: git repositories isolated
//! from the host's configuration, contract specs, plan reports, criterion
//! results for the dry-run executor, and a stage work directory holding
//! failed freeze jobs and their saved results.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use chrono::Utc;
use loom::commands::plan::verify::report::{verify_text, VerifyReport};
use loom::host_verify::{store, CriterionOutcome, JobKind, JobRecord, JobResult, JobState};
use loom::models::stage::{Stage, StageStatus};
use loom::plan::schema::ContractSpec;
use loom::verify::contracts::refusal::{daemon_refusal, DaemonRefusal};
use loom::verify::criteria::CriterionResult;
use tempfile::TempDir;

/// Git isolated from the host's global and system configuration.
pub fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", root.join(".loom-test-no-global"))
        .env("GIT_CONFIG_SYSTEM", root.join(".loom-test-no-system"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// A git repository on branch `main` with a local identity; `root` is canonical.
pub struct Repo {
    _tmp: TempDir,
    pub root: PathBuf,
}

impl Repo {
    pub fn init() -> Self {
        let tmp = TempDir::new().expect("create the repository directory");
        let root = tmp
            .path()
            .canonicalize()
            .expect("canonical repository root");
        git(&root, &["init", "-q", "-b", "main"]);
        git(&root, &["config", "user.email", "t@t.com"]);
        git(&root, &["config", "user.name", "t"]);
        git(&root, &["config", "commit.gpgsign", "false"]);
        Self { _tmp: tmp, root }
    }

    /// Write `body` at `path` (relative to the root), creating its parents.
    pub fn write(&self, path: &str, body: &str) {
        let full = self.root.join(path);
        fs::create_dir_all(full.parent().expect("the file has a parent")).expect("create dirs");
        fs::write(&full, body).expect("write the file");
    }

    /// A symbolic link at `link` (relative to the root) pointing at `target`.
    pub fn symlink(&self, link: &str, target: &str) {
        let full = self.root.join(link);
        fs::create_dir_all(full.parent().expect("the link has a parent")).expect("create dirs");
        std::os::unix::fs::symlink(target, &full).expect("create the symlink");
    }

    /// Commit everything in the working tree; returns the new HEAD.
    pub fn commit(&self, message: &str) -> String {
        git(&self.root, &["add", "-A"]);
        git(
            &self.root,
            &["commit", "-q", "--allow-empty", "-m", message],
        );
        git(&self.root, &["rev-parse", "HEAD"])
    }
}

/// A minimal Cargo manifest, the cargo-test adapter's run-root marker.
pub const CARGO_TOML: &str = "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\n";

/// A contract spec with no test hint, no runner and no cwd.
pub fn spec(id: &str, file: &str) -> ContractSpec {
    ContractSpec {
        id: id.to_string(),
        file: file.to_string(),
        test: None,
        runner: None,
        cwd: None,
        scenario: format!("the scenario of {id}"),
        rejects: format!("the wrong implementation {id} rejects"),
    }
}

/// `loom plan verify` over `text`, read as `PLAN.md` with no repository.
pub fn verify(text: &str) -> VerifyReport {
    verify_text(None, Path::new("PLAN.md"), text)
}

/// Every warning of `report`, across its categories.
pub fn all_warnings(report: &VerifyReport) -> Vec<String> {
    let sections = report.warnings.sections();
    sections
        .iter()
        .flat_map(|(_, items)| items.to_vec())
        .collect()
}

/// A repository with `text` committed as `PLAN.md`, the plan's path, and the
/// report `verify_text` gives for it, which has no errors.
pub fn committed_plan(text: &str) -> (Repo, PathBuf, VerifyReport) {
    let repo = Repo::init();
    repo.write("PLAN.md", text);
    repo.commit("plan");
    let path = repo.root.join("PLAN.md");
    let report = verify_text(None, &path, text);
    let errors: Vec<&str> = report.errors.iter().map(|e| e.message.as_str()).collect();
    assert!(
        errors.is_empty(),
        "the fixture plan must validate: {errors:?}"
    );
    (repo, path, report)
}

fn result(command: &str, exit: Option<i32>, stdout: &str, stderr: &str) -> CriterionResult {
    let success = exit == Some(0);
    let duration = Duration::from_millis(5);
    CriterionResult::new(
        command.to_string(),
        success,
        stdout.to_string(),
        stderr.to_string(),
        exit,
        duration,
        false,
    )
}

/// A command that exited 0.
pub fn passed(command: &str) -> CriterionResult {
    result(command, Some(0), "", "")
}

/// A command that exited `code` printing `stderr`.
pub fn exited(command: &str, code: i32, stderr: &str) -> CriterionResult {
    result(command, Some(code), "", stderr)
}

/// A command that exited `code` printing `stdout`.
pub fn exited_stdout(command: &str, code: i32, stdout: &str) -> CriterionResult {
    result(command, Some(code), stdout, "")
}

/// A command killed at its timeout.
pub fn timed_out(command: &str) -> CriterionResult {
    let duration = Duration::from_secs(300);
    CriterionResult::new(
        command.to_string(),
        false,
        String::new(),
        String::new(),
        None,
        duration,
        true,
    )
}

/// The stage whose freeze jobs [`Jobs`] holds.
pub const STAGE: &str = "stage-a";

/// The summary of a failed format outcome naming the writer's file.
pub const FORMAT_SUMMARY: &str = "cargo fmt --check: Diff in tests/a.rs at line 3";

/// One outcome of a freeze job's result.
pub fn outcome(label: &str, passed: bool, summary: &str) -> CriterionOutcome {
    CriterionOutcome {
        label: label.to_string(),
        passed,
        summary: summary.to_string(),
        log: None,
    }
}

/// A run whose contract was red and whose only failure is the format gate.
pub fn format_only() -> Vec<CriterionOutcome> {
    vec![
        outcome("contract:c1", true, "c1 failed as the contract requires"),
        outcome("format", false, FORMAT_SUMMARY),
    ]
}

/// The 32-hex-digit job id of the job submitted `order`th.
pub fn job_id(order: u64) -> String {
    format!("{order:032x}")
}

/// A stage work directory with `STAGE` executing and contract `c1` declared.
pub struct Jobs {
    _tmp: TempDir,
    pub work: PathBuf,
}

impl Jobs {
    pub fn create() -> Self {
        let tmp = TempDir::new().expect("create the work directory");
        let work = tmp
            .path()
            .canonicalize()
            .expect("canonical work dir")
            .join("work");
        fs::create_dir_all(&work).expect("create the work directory");
        let contract = ContractSpec {
            test: Some("c1".to_string()),
            runner: Some("cargo-test".to_string()),
            ..spec("c1", "tests/a.rs")
        };
        let stage = Stage {
            id: STAGE.to_string(),
            name: STAGE.to_string(),
            status: StageStatus::Executing,
            plan_version: 2,
            worktree: Some(STAGE.to_string()),
            working_dir: Some(".".to_string()),
            contracts: vec![contract],
            ..Default::default()
        };
        loom::verify::transitions::save_stage(&stage, &work).expect("save the stage");
        Self { _tmp: tmp, work }
    }

    /// A failed freeze job of `session` submitted `order`th, whose saved
    /// result holds `criteria` and `error`.
    pub fn failed(
        &self,
        session: &str,
        order: u64,
        criteria: Vec<CriterionOutcome>,
        error: Option<&str>,
    ) -> JobRecord {
        let record = job_record(session, order);
        let result = JobResult {
            job_id: record.id.clone(),
            head: record.head.clone(),
            passed: false,
            criteria,
            error: error.map(str::to_string),
            policy_hash: None,
        };
        store::save_result(&self.work, STAGE, &result).expect("save the job result");
        record
    }

    /// The refusal on record for `STAGE`.
    pub fn refusal(&self) -> DaemonRefusal {
        daemon_refusal(&self.work, STAGE)
            .expect("read the refusal")
            .expect("a refusal is recorded")
    }
}

fn job_record(session: &str, order: u64) -> JobRecord {
    let now = Utc::now();
    JobRecord {
        id: job_id(order),
        submission_order: order,
        stage_id: STAGE.to_string(),
        session_id: session.to_string(),
        kind: JobKind::Freeze,
        head: "0123456789abcdef0123456789abcdef01234567".to_string(),
        tree_digest: "tree-digest".to_string(),
        no_cache: false,
        criteria: None,
        state: JobState::Failed,
        requests: Vec::new(),
        queued_at: now,
        started_at: Some(now),
        finished_at: Some(now),
        pid: None,
        detail: None,
    }
}
