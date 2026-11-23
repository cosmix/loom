//! Contracts for a spawn that fails or is refused after the stage was read:
//! a failed backend spawn keeps every byte of the retry's worktree, and a
//! hold filed while the worktree was being provisioned stops the launch.

use std::path::Path;

use tempfile::TempDir;

use crate::fs::session_files::save_session;
use crate::fs::work_dir::write_terminal_config;
use crate::git::get_or_create_worktree;
use crate::models::session::{Session, SessionBackendKind, TerminalConfig};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::core::{Orchestrator, OrchestratorConfig};
use crate::orchestrator::scheduling_report::BlockReason;
use crate::plan::{schema::StageDefinition, ExecutionGraph};
use crate::test_git::{commit_file, git, TestRepo};
use crate::verify::transitions::{load_stage, save_stage};

/// A repository on `main` holding `seed.txt`, with the state and worktree
/// directories excluded from its status as in a real project.
fn repo() -> TempDir {
    let repo = TestRepo::builder()
        .identity("t", "t@t.com")
        .no_seed()
        .create()
        .into_temp_dir();
    let root = repo.path();
    std::fs::write(root.join(".git/info/exclude"), ".loom/\n.worktrees/\n").unwrap();
    commit_file(root, "seed.txt", "seed", "seed.txt");
    std::fs::create_dir_all(root.join(".loom").join("work")).unwrap();
    repo
}

fn graph(ids: &[&str]) -> ExecutionGraph {
    let definitions = ids.iter().map(|id| StageDefinition {
        id: id.to_string(),
        name: id.to_string(),
        working_dir: ".".into(),
        ..Default::default()
    });
    ExecutionGraph::build(definitions.collect()).unwrap()
}

/// An orchestrator over `root` on the tmux lane, so building it never probes
/// the host for a terminal emulator.
fn orchestrator(root: &Path, graph: ExecutionGraph) -> Orchestrator {
    let work_dir = root.join(".loom").join("work");
    std::fs::create_dir_all(&work_dir).unwrap();
    let backend = SessionBackendKind::Tmux;
    write_terminal_config(&work_dir, &TerminalConfig { backend }).unwrap();
    let config = OrchestratorConfig {
        work_dir,
        repo_root: root.to_path_buf(),
        base_branch: Some("main".to_string()),
        enable_skill_routing: false,
        ..Default::default()
    };
    Orchestrator::new(config, graph).unwrap()
}

fn stage(id: &str, status: StageStatus, session: Option<&str>) -> Stage {
    Stage {
        id: id.to_string(),
        name: id.to_string(),
        status,
        session: session.map(str::to_string),
        ..Stage::default()
    }
}

/// A retry's worktree whose only work is a staged edit and an untracked file
/// (no commit ahead of `main`) survives a failed backend spawn intact.
#[test]
fn a_failed_spawn_keeps_staged_and_untracked_work() {
    let repo = repo();
    let root = repo.path();
    let main_tip = git(root, &["rev-parse", "main"]);
    let worktree = get_or_create_worktree("s", root, Some("main"), None).unwrap();
    let tree = worktree.path.clone();
    std::fs::write(tree.join("seed.txt"), "staged edit").unwrap();
    git(&tree, &["add", "seed.txt"]);
    std::fs::write(tree.join("notes.txt"), "untracked notes").unwrap();
    let staged_before = git(&tree, &["diff", "--cached"]);
    let mut orchestrator = orchestrator(root, graph(&["s"]));
    let work_dir = orchestrator.config.work_dir.clone();
    save_stage(&stage("s", StageStatus::Executing, Some("sess")), &work_dir).unwrap();

    orchestrator.block_failed_spawn("s", "sess", &anyhow::anyhow!("boom"));

    assert!(tree.is_dir(), "the failed spawn removed the worktree");
    assert_eq!(
        git(&tree, &["diff", "--cached"]),
        staged_before,
        "the staged edit was lost"
    );
    assert_eq!(
        std::fs::read(tree.join("notes.txt")).ok().as_deref(),
        Some(b"untracked notes".as_slice()),
        "the untracked file was lost"
    );
    assert_eq!(git(root, &["rev-parse", "refs/heads/loom/s"]), main_tip);
    assert_eq!(
        load_stage("s", &work_dir).unwrap().status,
        StageStatus::Blocked
    );
}

/// A hold filed after `start_stage` read a `Queued` stage reaches
/// `mark_agent_executing` with the write-ahead record saved: the stage must
/// not launch, and the record must be undone.
#[test]
fn a_hold_taken_during_provisioning_prevents_the_launch() {
    let temp = tempfile::tempdir().unwrap();
    let mut orchestrator = orchestrator(temp.path(), graph(&["alpha"]));
    let work_dir = orchestrator.config.work_dir.clone();
    let mut queued = stage("alpha", StageStatus::Queued, None);
    queued.held = true;
    save_stage(&queued, &work_dir).unwrap();
    let mut session = Session::new();
    session.assign_to_stage("alpha".to_string());
    save_session(&session, &work_dir).unwrap();
    let record = work_dir.join("sessions").join(format!("{}.md", session.id));
    assert!(record.exists(), "the write-ahead record was not saved");

    let marked = orchestrator
        .mark_agent_executing("alpha", &session.id)
        .unwrap();

    assert!(marked.is_none(), "a held stage was marked executing");
    let after = load_stage("alpha", &work_dir).unwrap();
    assert_eq!(after.status, StageStatus::Queued);
    assert!(after.held, "the hold was dropped");
    assert_eq!(after.session, None);
    assert!(!record.exists(), "the write-ahead record was left behind");
    assert_eq!(
        orchestrator.spawn_blocks.get("alpha"),
        Some(&BlockReason::Held)
    );
}
