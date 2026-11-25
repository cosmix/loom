use super::resolver_spawn::test_fixtures::{orchestrator_with_conflict, repo_with_stage_branches};
use super::Orchestrator;
use crate::fs::session_files::{load_session_exact, save_session};
use crate::git::MergeBlock;
use crate::models::failure::FailureType;
use crate::models::session::{Session, SessionExitReason, SessionStatus, SessionType};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::core::OrchestratorConfig;
use crate::plan::ExecutionGraph;
use crate::test_git::{git, git_unchecked, TestRepo};
use serial_test::serial;

#[test]
fn merge_probe_failure_does_not_consume_resolver_attempt_budget() {
    // `loom/probe` shares no history with `main`, so the merge-tree probe that
    // lists the conflicting files fails; both branches and the worktree exist.
    let repo = repo_with_stage_branches(&[]);
    let root = repo.path();
    git(root, &["checkout", "-q", "--orphan", "loom/probe"]);
    git(root, &["commit", "-q", "-m", "unrelated root"]);
    git(root, &["checkout", "-q", "main"]);
    let mut orchestrator = orchestrator_with_conflict(root, "probe");
    let work_dir = orchestrator.config.work_dir.clone();

    assert_eq!(orchestrator.spawn_merge_resolution_sessions().unwrap(), 0);

    let stage = crate::verify::transitions::load_stage("probe", &work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::MergeConflict);
    assert_eq!(
        super::resolver_attempts::merge_resolver_attempts(&work_dir, "probe"),
        0
    );
    assert!(!super::attempts_file(&work_dir, "probe").exists());
}

struct FakeRetirementBackend {
    probe: Result<bool, &'static str>,
    kill: Result<(), &'static str>,
    confirm: Result<bool, &'static str>,
}

impl FakeRetirementBackend {
    fn probe(&self, _: &Session) -> anyhow::Result<bool> {
        self.probe.map_err(anyhow::Error::msg)
    }

    fn kill(&self, _: &Session) -> anyhow::Result<()> {
        self.kill.map_err(anyhow::Error::msg)
    }

    fn confirm(&self, _: &Session) -> anyhow::Result<bool> {
        self.confirm.map_err(anyhow::Error::msg)
    }
}

fn retirement_session(session_type: SessionType) -> Session {
    let mut session = Session::new();
    session.session_type = session_type;
    session.assign_to_stage("stale-writer".to_string());
    session.status = SessionStatus::Running;
    session
}

fn tracked_stage_session(orchestrator: &mut Orchestrator) -> (Session, std::path::PathBuf) {
    let session = retirement_session(SessionType::Stage);
    save_session(&session, &orchestrator.config.work_dir).unwrap();
    let signal = orchestrator
        .config
        .work_dir
        .join("signals")
        .join(format!("{}.md", session.id));
    std::fs::create_dir_all(signal.parent().unwrap()).unwrap();
    std::fs::write(&signal, "stale merge signal").unwrap();
    orchestrator
        .active_sessions
        .insert("stale-writer".to_string(), session.clone());
    (session, signal)
}

#[test]
fn stale_writer_probe_error_blocks_replacement() {
    let session = retirement_session(SessionType::Merge);
    let backend = FakeRetirementBackend {
        probe: Err("probe failed"),
        kill: Ok(()),
        confirm: Ok(true),
    };

    assert!(super::merge_gate::stale_merge_retirement_blocks_spawn(
        std::path::Path::new("/nonexistent/work"),
        &session,
        true,
        |session| backend.probe(session),
        |session| backend.kill(session),
        |session| backend.confirm(session),
    ));
}

#[test]
fn kill_failure_with_surviving_stale_writer_blocks_replacement() {
    let session = retirement_session(SessionType::Stage);
    let backend = FakeRetirementBackend {
        probe: Ok(false),
        kill: Err("kill failed"),
        confirm: Ok(false),
    };

    assert!(super::merge_gate::stale_merge_retirement_blocks_spawn(
        std::path::Path::new("/nonexistent/work"),
        &session,
        true,
        |session| backend.probe(session),
        |session| backend.kill(session),
        |session| backend.confirm(session),
    ));
}

#[test]
#[serial]
fn stale_writer_without_pid_identity_keeps_entry_and_signal() {
    let temp = tempfile::tempdir().unwrap();
    let work_dir = temp.path().join(".loom").join("work");
    let mut orchestrator = orchestrator_for(temp.path(), &work_dir);
    let (session, signal) = tracked_stage_session(&mut orchestrator);

    assert!(orchestrator.cleanup_stale_merge_session("stale-writer"));
    assert_eq!(orchestrator.active_sessions["stale-writer"].id, session.id);
    assert!(signal.exists());
    let persisted = load_session_exact(&work_dir, &session.id).unwrap().unwrap();
    assert_eq!(persisted.status, SessionStatus::Running);
    assert_eq!(persisted.exit_reason, None);
}

#[test]
#[serial]
fn confirmed_gone_stale_writer_is_replaced_and_allows_successor() {
    let temp = tempfile::tempdir().unwrap();
    let work_dir = temp.path().join(".loom").join("work");
    let mut orchestrator = orchestrator_for(temp.path(), &work_dir);
    let (session, signal) = tracked_stage_session(&mut orchestrator);
    let pid_dir = work_dir.join("pids");
    std::fs::create_dir_all(&pid_dir).unwrap();
    std::fs::write(
        pid_dir.join(format!("{}-{}.pid", session.tracking_key, session.id)),
        format!("{}\n{}\n", std::process::id(), u64::MAX),
    )
    .unwrap();

    assert!(!orchestrator.cleanup_stale_merge_session("stale-writer"));
    assert!(!orchestrator.active_sessions.contains_key("stale-writer"));
    assert!(!signal.exists());
    let persisted = load_session_exact(&work_dir, &session.id).unwrap().unwrap();
    assert_eq!(persisted.status, SessionStatus::ContextExhausted);
    assert_eq!(persisted.exit_reason, Some(SessionExitReason::Replaced));
}

/// Build a stage branch with work not in main, exercising the containment
/// guard used by the already-merged cleanup short circuit.
/// Build a repo whose `loom/<stage_id>` branch carries a commit that never
/// reached `main`, with its worktree still in place. When `with_extra_commit`
/// is `false`, the branch is created but left at the same commit as `main` —
/// used to test the phantom-merge zero-commits-ahead guard.
///
/// Returns the tempdir — which the caller must keep alive for the duration of
/// the test — and the worktree path.
fn repo_with_unmerged_stage_branch(
    stage_id: &str,
    with_extra_commit: bool,
) -> (tempfile::TempDir, std::path::PathBuf) {
    let temp = TestRepo::builder()
        .identity("t", "t@t.com")
        .seed("a.txt", "seed", "seed")
        .create()
        .into_temp_dir();
    let root = temp.path();

    let worktree_path = root.join(".worktrees").join(stage_id);
    let branch = format!("loom/{stage_id}");
    git(
        root,
        &[
            "worktree",
            "add",
            "-b",
            &branch,
            worktree_path.to_str().unwrap(),
        ],
    );
    if with_extra_commit {
        // Commit on the stage branch that never made it to `main` — the branch
        // is now provably ahead of the target.
        std::fs::write(worktree_path.join("b.txt"), "unmerged work").unwrap();
        git(&worktree_path, &["add", "b.txt"]);
        git(&worktree_path, &["commit", "-m", "unmerged work"]);
    }

    (temp, worktree_path)
}

/// Save a completed stage with the commit certified by its completion.
fn save_completed_unmerged_stage(stage_id: &str, work_dir: &std::path::Path) {
    let mut stage = Stage::new(stage_id.to_string(), None);
    stage.id = stage_id.to_string();
    stage.status = StageStatus::Completed;
    stage.merged = false;
    let root = crate::fs::work_dir::WorkDir::new(work_dir)
        .unwrap()
        .main_project_root()
        .unwrap();
    stage.completed_commit =
        Some(crate::git::get_branch_head(&format!("loom/{stage_id}"), &root).unwrap());
    crate::verify::transitions::save_stage(&stage, work_dir).unwrap();
}

/// Build an orchestrator over `root` targeting `main`, pinning the terminal env around `Orchestrator::new`.
fn orchestrator_for(root: &std::path::Path, work_dir: &std::path::Path) -> Orchestrator {
    let config = OrchestratorConfig {
        work_dir: work_dir.to_path_buf(),
        repo_root: root.to_path_buf(),
        base_branch: Some("main".to_string()),
        enable_skill_routing: false,
        ..Default::default()
    };
    let constructed = Orchestrator::new(config, ExecutionGraph::build(Vec::new()).unwrap());
    constructed.unwrap()
}

#[test]
#[serial]
fn already_merged_short_circuit_refuses_cleanup_for_unmerged_branch() {
    let stage_id = "unmerged-but-flagged";
    let (temp, worktree_path) = repo_with_unmerged_stage_branch(stage_id, true);
    let root = temp.path();
    let work_dir = root.join(".loom").join("work");
    let branch = format!("loom/{stage_id}");

    let mut stage = Stage::new(stage_id.to_string(), None);
    stage.id = stage_id.to_string();
    stage.status = StageStatus::Completed;
    stage.merged = true;
    crate::verify::transitions::save_stage(&stage, &work_dir).unwrap();

    let mut orchestrator = orchestrator_for(root, &work_dir);

    assert!(orchestrator.try_auto_merge(stage_id));

    assert!(
        worktree_path.exists(),
        "cleanup must refuse to remove the worktree: the branch holds commits \
         not provably in 'main'"
    );
    assert!(
        git_unchecked(
            root,
            &["rev-parse", "--verify", &format!("refs/heads/{branch}")]
        )
        .status
        .success(),
        "cleanup must refuse to delete the branch: it still holds unmerged commits"
    );
}

#[test]
#[serial]
fn failed_auto_merge_moves_completed_stage_to_merge_blocked() {
    let stage_id = "blocked-by-untracked-file";
    let (temp, _worktree_path) = repo_with_unmerged_stage_branch(stage_id, true);
    let root = temp.path();
    let work_dir = root.join(".loom").join("work");
    // Block the merge deterministically: an untracked b.txt in the main
    // checkout, with content different from the one committed on the stage
    // branch, makes `merge_stage` report an uncommitted overlap without
    // touching the checkout.
    std::fs::write(root.join("b.txt"), "conflicting untracked content").unwrap();
    save_completed_unmerged_stage(stage_id, &work_dir);
    let head_before = git_unchecked(root, &["rev-parse", "main"]).stdout;
    let mut orchestrator = orchestrator_for(root, &work_dir);

    assert!(!orchestrator.try_auto_merge(stage_id));

    let reloaded = crate::verify::transitions::load_stage(stage_id, &work_dir).unwrap();
    assert_eq!(reloaded.status, StageStatus::MergeBlocked);
    assert!(!reloaded.merged);
    let overlap = MergeBlock::UncommittedOverlap {
        paths: vec!["b.txt".to_string()],
    };
    assert_eq!(reloaded.merge.block, Some(overlap));
    let info = reloaded.failure_info.expect("a block records failure_info");
    assert_eq!(info.failure_type, FailureType::InfrastructureError);
    assert!(!info.evidence.is_empty(), "failure_info carries the reason");
    let head_after = git_unchecked(root, &["rev-parse", "main"]).stdout;
    assert_eq!(
        head_before, head_after,
        "a blocked merge must not move 'main'"
    );
    let branch_ref = format!("refs/heads/loom/{stage_id}");
    let branch = git_unchecked(root, &["rev-parse", "--verify", &branch_ref]);
    assert!(branch.status.success(), "the stage branch must survive");
}

#[test]
#[serial]
fn empty_stage_branch_routes_to_human_review() {
    let stage_id = "empty-stage-branch";
    let (temp, _worktree_path) = repo_with_unmerged_stage_branch(stage_id, false);
    let root = temp.path();
    let work_dir = root.join(".loom").join("work");

    save_completed_unmerged_stage(stage_id, &work_dir);

    let mut orchestrator = orchestrator_for(root, &work_dir);

    assert!(!orchestrator.try_auto_merge(stage_id));

    let reloaded = crate::verify::transitions::load_stage(stage_id, &work_dir).unwrap();
    assert_eq!(reloaded.status, StageStatus::NeedsHumanReview);
    assert!(!reloaded.merged);
    let review_reason = reloaded
        .review_reason
        .expect("routing to human review must record review_reason");
    assert!(
        review_reason.contains("zero commits"),
        "review_reason should explain the branch had zero commits: {review_reason}"
    );
}
