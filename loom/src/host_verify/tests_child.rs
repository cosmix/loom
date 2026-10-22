//! Tests for the host child (`child::run_job`). Each one that runs criteria
//! starts loom's real confinement and skips where it cannot start; every one
//! is serial, because the spawn observer is one process-wide slot.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use chrono::Utc;
use serial_test::serial;
use tempfile::TempDir;

use super::child::run_job;
use super::{digest, new_job_id, store, JobKind, JobRecord, JobResult, JobState};
use crate::models::stage::{AcceptanceCriterion, Stage, StageStatus, StageType};
use crate::process::sandbox_probe::{confined_commands_runnable, skip_unless};
use crate::verify::transitions::save_stage;

pub(super) const STAGE: &str = "stage-a";

pub(super) fn confinement_unavailable(test_name: &str) -> bool {
    skip_unless(
        confined_commands_runnable(),
        test_name,
        "loom's confinement cannot start here",
    )
}

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

/// A repository with one commit, the stage worktree `.worktrees/stage-a`,
/// the state root `<repo>/.loom/work` holding the stage, and a plan without
/// goal checks that the state root's `config.toml` names.
pub(super) struct Fixture {
    _dir: TempDir,
    repo: PathBuf,
    pub(super) work_dir: PathBuf,
    pub(super) worktree: PathBuf,
    stage_type: StageType,
}

impl Fixture {
    pub(super) fn new(acceptance: &[&str], stage_type: StageType) -> Self {
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
        let branch = format!("loom/{STAGE}");
        git(
            &repo,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                &branch,
                worktree.to_str().unwrap(),
            ],
        );
        let work_dir = repo.join(".loom").join("work");
        fs::create_dir_all(&work_dir).expect("create state root");
        let fixture = Self {
            _dir: dir,
            repo,
            work_dir,
            worktree,
            stage_type,
        };
        save_stage(&fixture.stage(acceptance), &fixture.work_dir).expect("save stage");
        fixture.write_plan();
        fixture
    }

    fn stage(&self, acceptance: &[&str]) -> Stage {
        Stage {
            id: STAGE.to_string(),
            name: STAGE.to_string(),
            status: StageStatus::Executing,
            worktree: Some(STAGE.to_string()),
            working_dir: Some(".".to_string()),
            plan_version: 1,
            stage_type: self.stage_type,
            acceptance: acceptance
                .iter()
                .map(|command| AcceptanceCriterion::Simple(command.to_string()))
                .collect(),
            ..Default::default()
        }
    }

    fn write_plan(&self) {
        let plan = self.work_dir.join("PLAN-host.md");
        let yaml = format!(
            "loom:\n  version: 1\n  stages:\n    - id: {STAGE}\n      name: {STAGE}\n      working_dir: .\n      acceptance:\n        - \"true\"\n"
        );
        fs::write(
            &plan,
            format!(
            "# Plan\n\n<!-- loom METADATA -->\n\n```yaml\n{yaml}```\n\n<!-- END loom METADATA -->\n"
        ),
        )
        .expect("write plan");
        let config = format!(
            "[plan]\nsource_path = \"{}\"\nplan_id = \"host\"\nplan_name = \"host\"\nbase_branch = \"main\"\n",
            plan.display()
        );
        fs::write(self.work_dir.join("config.toml"), config).expect("write config.toml");
    }

    /// The checkout the job verifies: the worktree, or the repository root
    /// for a knowledge stage.
    fn checkout(&self) -> &Path {
        match self.stage_type {
            StageType::Knowledge => &self.repo,
            _ => &self.worktree,
        }
    }

    fn job_head(&self) -> String {
        git(self.checkout(), &["rev-parse", "HEAD"])
    }

    /// Save a current job record for the checkout as it stands now.
    pub(super) fn job(&self, state: JobState, no_cache: bool) -> JobRecord {
        let tree_digest = match self.stage_type {
            StageType::Knowledge => digest::knowledge_tree_digest(&self.repo),
            _ => digest::tree_digest(&self.worktree),
        }
        .expect("tree digest");
        let now = Utc::now();
        let record = JobRecord {
            id: new_job_id(),
            stage_id: STAGE.to_string(),
            session_id: "session-a".to_string(),
            kind: JobKind::Check,
            head: self.job_head(),
            tree_digest,
            no_cache,
            criteria: None,
            state,
            requests: vec!["r1".to_string()],
            queued_at: now,
            started_at: (state == JobState::Running).then_some(now),
            finished_at: None,
            pid: None,
            detail: None,
        };
        store::save_current(&self.work_dir, &record).expect("save current record");
        record
    }

    pub(super) fn run(&self, record: &JobRecord) -> JobResult {
        let result = run_job(&self.work_dir, STAGE, &record.id).expect("run_job");
        let saved = store::load_result(&self.work_dir, STAGE, &record.id).expect("load result");
        assert_eq!(
            saved.as_ref(),
            Some(&result),
            "result.json matches the return"
        );
        result
    }
}

pub(super) fn labels_and_verdicts(result: &JobResult) -> Vec<(&str, bool)> {
    result
        .criteria
        .iter()
        .map(|outcome| (outcome.label.as_str(), outcome.passed))
        .collect()
}

fn error_of(result: &JobResult) -> &str {
    assert!(!result.passed, "an errored result never passes");
    result.error.as_deref().expect("an error")
}

#[test]
#[serial]
fn each_criterion_is_an_outcome_and_a_progress_line() {
    if confinement_unavailable("each_criterion_is_an_outcome_and_a_progress_line") {
        return;
    }
    let fx = Fixture::new(&["true", "false"], StageType::Standard);
    let record = fx.job(JobState::Running, false);
    let result = fx.run(&record);
    assert_eq!(
        labels_and_verdicts(&result),
        [("acceptance-0", true), ("acceptance-1", false)]
    );
    assert!(!result.passed);
    assert_eq!(result.error, None);
    assert_eq!(result.head, record.head);
    let progress = store::read_progress(&fx.work_dir, STAGE, &record.id).unwrap();
    assert_eq!(progress, result.criteria);
}

#[test]
#[serial]
fn a_long_failure_has_a_short_summary_and_a_whole_log() {
    if confinement_unavailable("a_long_failure_has_a_short_summary_and_a_whole_log") {
        return;
    }
    let fx = Fixture::new(&["seq 1 5000; exit 1"], StageType::Standard);
    let record = fx.job(JobState::Running, false);
    let result = fx.run(&record);
    let outcome = &result.criteria[0];
    assert!(!outcome.passed);
    assert!(
        outcome.summary.len() <= 8 * 1024,
        "{}",
        outcome.summary.len()
    );
    assert_eq!(outcome.log.as_deref(), Some("acceptance-0.log"));
    let job_dir = store::job_dir(&fx.work_dir, STAGE, &record.id).unwrap();
    let log = fs::read_to_string(job_dir.join("acceptance-0.log")).unwrap();
    let lines: Vec<&str> = log.lines().collect();
    assert_eq!(lines.len(), 5000, "the log holds every line");
    assert_eq!((lines[0], lines[4999]), ("1", "5000"));
}

#[test]
#[serial]
fn a_head_other_than_the_request_is_an_error_result() {
    if confinement_unavailable("a_head_other_than_the_request_is_an_error_result") {
        return;
    }
    let fx = Fixture::new(&["true"], StageType::Standard);
    let mut record = fx.job(JobState::Running, false);
    record.head = "0".repeat(40);
    store::save_current(&fx.work_dir, &record).unwrap();
    let result = fx.run(&record);
    let error = error_of(&result);
    assert!(error.contains(&record.head), "{error}");
    assert!(error.contains(&fx.job_head()), "{error}");
    assert!(result.criteria.is_empty(), "nothing ran");
}

#[test]
#[serial]
fn a_tree_written_during_the_run_is_an_error_result() {
    if confinement_unavailable("a_tree_written_during_the_run_is_an_error_result") {
        return;
    }
    let fx = Fixture::new(&["echo changed >> .gitignore"], StageType::Standard);
    let record = fx.job(JobState::Running, false);
    let result = fx.run(&record);
    let error = error_of(&result);
    assert!(error.contains("worktree changed during the run"), "{error}");
    assert!(error.contains("tree digest"), "{error}");
}

#[test]
#[serial]
fn a_clean_commit_during_the_run_is_an_error_naming_head() {
    if confinement_unavailable("a_clean_commit_during_the_run_is_an_error_naming_head") {
        return;
    }
    let gate = TempDir::new().unwrap();
    let go = gate.path().join("go");
    // Bounded, so a confinement that hides the gate fails the criterion
    // rather than hanging until the command timeout.
    let wait = format!(
        "i=0; while [ ! -f {go} ] && [ $i -lt 300 ]; do sleep 0.1; i=$((i+1)); done",
        go = go.display()
    );
    let fx = Fixture::new(&[wait.as_str()], StageType::Standard);
    let record = fx.job(JobState::Running, false);
    let (work_dir, job_id) = (fx.work_dir.clone(), record.id.clone());
    let job = std::thread::spawn(move || run_job(&work_dir, STAGE, &job_id));
    let started = || {
        !store::load_groups(&fx.work_dir, STAGE, &record.id)
            .unwrap()
            .is_empty()
    };
    assert!(wait_until(started), "the criterion never started");
    git(
        &fx.worktree,
        &["commit", "-q", "--allow-empty", "-m", "moved"],
    );
    fs::write(&go, "").unwrap();
    let result = job.join().expect("job thread").expect("run_job");
    let error = error_of(&result);
    assert!(error.contains("worktree changed during the run"), "{error}");
    assert!(error.contains("HEAD"), "{error}");
    assert!(
        !error.contains("tree digest"),
        "a clean commit keeps the digest: {error}"
    );
}

fn wait_until(mut check: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(20) {
        if check() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    check()
}

#[test]
#[serial]
fn a_knowledge_stage_runs_in_the_repository_root() {
    if confinement_unavailable("a_knowledge_stage_runs_in_the_repository_root") {
        return;
    }
    // `.worktrees` exists in the repository root only, never in a worktree.
    let fx = Fixture::new(&["test -d .worktrees"], StageType::Knowledge);
    let record = fx.job(JobState::Running, false);
    let result = fx.run(&record);
    assert!(result.passed, "{result:?}");
    assert_eq!(labels_and_verdicts(&result), [("acceptance-0", true)]);
    assert_eq!(result.head, git(&fx.repo, &["rev-parse", "HEAD"]));
}

#[test]
#[serial]
fn the_spawn_observer_records_groups_only_during_the_job() {
    if confinement_unavailable("the_spawn_observer_records_groups_only_during_the_job") {
        return;
    }
    let fx = Fixture::new(&["true"], StageType::Standard);
    let record = fx.job(JobState::Running, false);
    assert!(fx.run(&record).passed);
    let groups = store::load_groups(&fx.work_dir, STAGE, &record.id).unwrap();
    assert!(!groups.is_empty(), "the criterion's group was recorded");
    let stage = fx.stage(&["true"]);
    crate::verify::criteria::run_acceptance(&stage, Some(fx.worktree.as_path())).unwrap();
    let after = store::load_groups(&fx.work_dir, STAGE, &record.id).unwrap();
    assert_eq!(after, groups, "the observer was cleared when the job ended");
}

#[test]
#[serial]
fn a_no_cache_job_runs_a_criterion_the_cache_would_answer() {
    if confinement_unavailable("a_no_cache_job_runs_a_criterion_the_cache_would_answer") {
        return;
    }
    let fx = Fixture::new(&["test -f .gitignore"], StageType::Standard);
    assert!(fx.run(&fx.job(JobState::Running, false)).passed);
    let cached = fx.run(&fx.job(JobState::Running, false));
    let outcome = &cached.criteria[0];
    assert!(outcome.summary.contains("(cached)"), "{}", outcome.summary);
    assert_eq!(outcome.log, None, "a cached answer has no output");

    let fresh = fx.run(&fx.job(JobState::Running, true));
    let outcome = &fresh.criteria[0];
    assert!(fresh.passed);
    assert!(!outcome.summary.contains("(cached)"), "{}", outcome.summary);
    assert_eq!(outcome.log.as_deref(), Some("acceptance-0.log"));
}

#[test]
#[serial]
fn a_job_that_is_not_running_is_refused_without_a_result() {
    let fx = Fixture::new(&["true"], StageType::Standard);
    let record = fx.job(JobState::Queued, false);
    assert!(run_job(&fx.work_dir, STAGE, &record.id).is_err());
    let saved = store::load_result(&fx.work_dir, STAGE, &record.id).unwrap();
    assert_eq!(saved, None);
}

#[test]
#[serial]
fn a_state_root_without_a_plan_source_is_an_error_result() {
    let fx = Fixture::new(&["true"], StageType::Standard);
    fs::remove_file(fx.work_dir.join("config.toml")).unwrap();
    let record = fx.job(JobState::Running, false);
    let result = fx.run(&record);
    let error = error_of(&result);
    assert!(error.contains("plan source"), "{error}");
    assert!(result.criteria.is_empty(), "nothing ran");
}
