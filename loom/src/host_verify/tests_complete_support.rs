//! The fixture the Complete-job tests share (`tests_apply*`,
//! `tests_child_complete`): a repository with one commit, the stage worktree
//! `.worktrees/stage-a` on `loom/stage-a` (none for a knowledge stage), the
//! state root `<repo>/.loom/work` holding the stage, its running session, the
//! session's signal file and a plan the state root's `config.toml` names.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::Utc;
use tempfile::TempDir;

use super::apply::{Applied, APPLIED_MARKER};
use super::{apply, digest, new_job_id, store};
use super::{CriterionOutcome, JobKind, JobRecord, JobResult, JobState};
use crate::fs::session_files::{load_session_exact, save_session};
use crate::handoff::completion::policy::effective_policy_hash;
use crate::handoff::{
    load_session_checkpoint, CompletionAttemptEvidence, CompletionCheckpoint, CompletionPhase,
};
use crate::models::session::{Session, SessionStatus, SessionType};
use crate::models::stage::{AcceptanceCriterion, Stage, StageStatus, StageType};
use crate::verify::transitions::{load_stage, save_stage, update_stage};

pub(super) const STAGE: &str = "stage-a";
pub(super) const SESSION: &str = "session-a";

/// Run git in `dir` without the user's or the system's configuration.
pub(super) fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// The reason of an `Ignored` outcome; any other outcome fails the test.
pub(super) fn ignored(applied: Applied) -> String {
    match applied {
        Applied::Ignored(reason) => reason,
        other => panic!("expected Ignored, got {other:?}"),
    }
}

pub(super) struct Fixture {
    _dir: TempDir,
    pub(super) repo: PathBuf,
    pub(super) work_dir: PathBuf,
    pub(super) worktree: PathBuf,
    stage_type: StageType,
    plan_version: u32,
}

impl Fixture {
    pub(super) fn new(stage_type: StageType, plan_version: u32) -> Self {
        let dir = TempDir::new().expect("temp dir");
        let repo = dir.path().canonicalize().expect("canonical temp dir");
        git(&repo, &["init", "-q", "-b", "main"]);
        for (key, value) in [
            ("user.email", "test@test.com"),
            ("user.name", "Test User"),
            ("commit.gpgsign", "false"),
        ] {
            git(&repo, &["config", key, value]);
        }
        fs::write(repo.join(".gitignore"), ".loom/\n.worktrees/\n").expect("write .gitignore");
        git(&repo, &["add", ".gitignore"]);
        git(&repo, &["commit", "-q", "-m", "initial"]);
        let worktree = repo.join(".worktrees").join(STAGE);
        if stage_type != StageType::Knowledge {
            let branch = format!("loom/{STAGE}");
            let path = worktree.to_str().unwrap();
            git(&repo, &["worktree", "add", "-q", "-b", &branch, path]);
        }
        let work_dir = repo.join(".loom").join("work");
        fs::create_dir_all(work_dir.join("signals")).expect("create state root");
        let fixture = Self {
            _dir: dir,
            repo,
            work_dir,
            worktree,
            stage_type,
            plan_version,
        };
        save_stage(&fixture.stage(), &fixture.work_dir).expect("save stage");
        fixture.write_plan();
        fixture.write_session();
        fixture
    }

    fn stage(&self) -> Stage {
        let knowledge = self.stage_type == StageType::Knowledge;
        Stage {
            id: STAGE.to_string(),
            name: STAGE.to_string(),
            status: StageStatus::Executing,
            session: Some(SESSION.to_string()),
            worktree: (!knowledge).then(|| STAGE.to_string()),
            working_dir: Some(".".to_string()),
            plan_version: self.plan_version,
            stage_type: self.stage_type,
            acceptance: vec![AcceptanceCriterion::Simple("true".to_string())],
            ..Default::default()
        }
    }

    fn plan_path(&self) -> PathBuf {
        self.work_dir.join("PLAN-host.md")
    }

    fn write_plan(&self) {
        let yaml = format!(
            "loom:\n  version: 1\n  stages:\n    - id: {STAGE}\n      name: {STAGE}\n      working_dir: .\n      acceptance:\n        - \"true\"\n"
        );
        let plan = self.plan_path();
        let body = format!(
            "# Plan\n\n<!-- loom METADATA -->\n\n```yaml\n{yaml}```\n\n<!-- END loom METADATA -->\n"
        );
        fs::write(&plan, body).expect("write plan");
        let config = format!(
            "[plan]\nsource_path = \"{}\"\nplan_id = \"host\"\nplan_name = \"host\"\nbase_branch = \"main\"\n",
            plan.display()
        );
        fs::write(self.work_dir.join("config.toml"), config).expect("write config.toml");
    }

    /// The running session that owns the stage, and its signal file.
    fn write_session(&self) {
        let mut session = Session::new();
        session.id = SESSION.to_string();
        session.session_type = match self.stage_type {
            StageType::Knowledge => SessionType::Knowledge,
            _ => SessionType::Stage,
        };
        session.assign_to_stage(STAGE.to_string());
        session.status = SessionStatus::Running;
        save_session(&session, &self.work_dir).expect("save session");
        fs::write(self.signal_path(), "# Signal\n").expect("write signal");
    }

    fn signal_path(&self) -> PathBuf {
        self.work_dir.join("signals").join(format!("{SESSION}.md"))
    }

    /// The checkout a job verifies: the worktree, or the repository root for
    /// a knowledge stage.
    fn checkout(&self) -> &Path {
        match self.stage_type {
            StageType::Knowledge => &self.repo,
            _ => &self.worktree,
        }
    }

    pub(super) fn head(&self) -> String {
        git(self.checkout(), &["rev-parse", "HEAD"])
    }

    /// Move HEAD (and the stage branch) with an empty commit; returns it.
    pub(super) fn commit_empty(&self) -> String {
        git(
            self.checkout(),
            &["commit", "-q", "--allow-empty", "-m", "moved"],
        );
        self.head()
    }

    /// Save a current job record of `kind` for the checkout as it stands now,
    /// finished when `state` is terminal.
    pub(super) fn record(&self, kind: JobKind, state: JobState) -> JobRecord {
        let tree_digest = match self.stage_type {
            StageType::Knowledge => digest::knowledge_tree_digest(&self.repo),
            _ => digest::tree_digest(&self.worktree),
        }
        .expect("tree digest");
        let now = Utc::now();
        let record = JobRecord {
            id: new_job_id(),
            stage_id: STAGE.to_string(),
            session_id: SESSION.to_string(),
            kind,
            head: self.head(),
            tree_digest,
            no_cache: false,
            criteria: None,
            state,
            requests: vec!["r1".to_string()],
            queued_at: now,
            started_at: Some(now),
            finished_at: state.is_terminal().then_some(now),
            pid: None,
            detail: None,
        };
        store::save_current(&self.work_dir, &record).expect("save current record");
        record
    }

    /// A finished job: its record, and a result with one outcome per entry of
    /// `outcomes` (none when empty) bound to the stage's current policy.
    pub(super) fn finished(
        &self,
        kind: JobKind,
        state: JobState,
        outcomes: &[(&str, bool)],
    ) -> JobRecord {
        let record = self.record(kind, state);
        if !outcomes.is_empty() {
            let result = JobResult {
                job_id: record.id.clone(),
                head: record.head.clone(),
                passed: outcomes.iter().all(|(_, passed)| *passed),
                criteria: outcomes
                    .iter()
                    .map(|(label, passed)| outcome(label, *passed))
                    .collect(),
                error: None,
                policy_hash: Some(self.policy()),
            };
            store::save_result(&self.work_dir, STAGE, &result).expect("save result");
        }
        record
    }

    /// A job that passed one acceptance criterion.
    pub(super) fn passed(&self) -> JobRecord {
        self.finished(
            JobKind::Complete,
            JobState::Passed,
            &[("acceptance-0", true)],
        )
    }

    pub(super) fn policy(&self) -> String {
        effective_policy_hash(&self.load_stage(), &self.work_dir).expect("policy hash")
    }

    /// Amend the plan after a job ran, so the policy its result names is stale.
    pub(super) fn amend_policy(&self) {
        let mut plan = fs::read_to_string(self.plan_path()).expect("read plan");
        plan.push_str("\nAmended after the run.\n");
        fs::write(self.plan_path(), plan).expect("write plan");
    }

    pub(super) fn load_stage(&self) -> Stage {
        load_stage(STAGE, &self.work_dir).expect("load stage")
    }

    /// Hand the stage to another session, as a handoff or a reset does.
    pub(super) fn hand_stage_to_successor(&self) {
        update_stage(STAGE, &self.work_dir, |stage| {
            stage.session = Some("session-b".to_string());
            Ok(())
        })
        .expect("hand the stage over");
    }

    pub(super) fn apply(&self, record: &JobRecord) -> anyhow::Result<Applied> {
        apply::apply_finished(&self.work_dir, record)
    }

    pub(super) fn marked(&self, record: &JobRecord) -> bool {
        store::load_job_file(&self.work_dir, STAGE, &record.id, APPLIED_MARKER)
            .expect("read the applied marker")
            .is_some()
    }

    pub(super) fn checkpoint(&self) -> Option<CompletionCheckpoint> {
        load_session_checkpoint(STAGE, SESSION, &self.work_dir).expect("load checkpoint")
    }

    pub(super) fn latest(&self) -> CompletionAttemptEvidence {
        self.checkpoint()
            .expect("a completion checkpoint")
            .latest
            .expect("a latest attempt")
    }

    pub(super) fn session_status(&self) -> SessionStatus {
        load_session_exact(&self.work_dir, SESSION)
            .expect("load session")
            .expect("the session record")
            .status
    }

    pub(super) fn signal_exists(&self) -> bool {
        self.signal_path().exists()
    }

    /// The receipt of `record` is in the session's checkpoint.
    pub(super) fn receipt_names(&self, record: &JobRecord) -> bool {
        self.checkpoint()
            .and_then(|checkpoint| checkpoint.accepted)
            .is_some_and(|receipt| {
                receipt.evidence_nonce == record.id && receipt.commit == record.head
            })
    }

    /// The latest attempt is the daemon's refusal of `record`, and the stage
    /// is untouched.
    pub(super) fn assert_rejected(&self, record: &JobRecord) -> CompletionAttemptEvidence {
        let latest = self.latest();
        assert_eq!(latest.phase, CompletionPhase::DaemonRejected);
        assert_eq!(
            latest.external_failure_code.as_deref(),
            Some("daemon_rejected")
        );
        assert_eq!(latest.evidence_nonce, record.id);
        assert_eq!(latest.commit, record.head);
        assert!(
            latest.diagnostic_first_line.is_some(),
            "the refusal names a binding"
        );
        let stage = self.load_stage();
        assert_eq!(stage.status, StageStatus::Executing);
        assert_eq!(stage.completed_commit, None);
        assert!(!self.receipt_names(record), "a refused run has no receipt");
        latest
    }
}

fn outcome(label: &str, passed: bool) -> CriterionOutcome {
    let summary = if passed {
        format!("✓ {label}")
    } else {
        format!("✗ {label}: boom")
    };
    CriterionOutcome {
        label: label.to_string(),
        passed,
        summary,
        log: None,
    }
}
