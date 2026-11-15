//! Contracts for the daemon side of the target guard: a move of `main` that
//! loom did not make holds every merge into it until the operator accepts or
//! restores it, while attested operator commits and knowledge-only commits
//! let merges continue. Driven through the daemon's own entry points with
//! real git.

use std::path::Path;
use std::time::Duration;

use super::merge_lifecycle_e2e_tests_support::*;
use crate::git::hooks::install_reference_transaction_hook;
use crate::git::merge::lock::MergeLock;
use crate::git::target_guard::{
    accept, accepted_tip, attestation_mode, recorded_hold, AttestationMode, Hold,
};
use crate::git::MergeBlock;
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::core::recovery::Recovery;
use crate::orchestrator::core::Orchestrator;
use crate::orchestrator::scheduling_report::{alerts, Severity};
use crate::plan::schema::StageDefinition;
use crate::plan::ExecutionGraph;
use crate::test_git::git_with_env;
use crate::verify::transitions::{save_stage, update_stage};

fn guard(orchestrator: &mut Orchestrator) -> Option<Hold> {
    orchestrator.check_target_guard()
}

fn hooked_repo() -> tempfile::TempDir {
    let repo = repo_with_worktree();
    install_reference_transaction_hook(repo.path()).unwrap();
    repo
}

fn assert_attestation_active(root: &Path, orchestrator: &Orchestrator) {
    let mode = attestation_mode(root, &orchestrator.config.work_dir);
    assert_eq!(mode, AttestationMode::Active);
}

/// Stage `s` Completed with one commit on `loom/s` recorded as its
/// `completed_commit`.
fn completed_with_work(root: &Path) -> Orchestrator {
    let orchestrator = orchestrator_with_completed_stage(root);
    commit_file(&worktree(root), "work.txt", "stage work");
    record_completed_commit(&orchestrator, root);
    orchestrator
}

fn record_completed_commit(orchestrator: &Orchestrator, root: &Path) {
    let tip = branch_tip(root);
    update_stage(ID, &orchestrator.config.work_dir, |stage| {
        stage.completed_commit = Some(tip);
        Ok(())
    })
    .unwrap();
}

/// A commit of `main`'s tree plus `name`, child of `main`, on no branch.
fn crafted_commit(root: &Path, name: &str, text: &str) -> String {
    let index = root.join(".git/crafted-index");
    let blob_file = root.join(".git/crafted-blob");
    std::fs::write(&blob_file, text).unwrap();
    let blob = git(root, &["hash-object", "-w", blob_file.to_str().unwrap()]);
    let indexed =
        |args: &[&str]| -> String { git_with_env(root, args, &[("GIT_INDEX_FILE", &index)]) };
    indexed(&["read-tree", "main"]);
    let info = format!("100644,{blob},{name}");
    indexed(&["update-index", "--add", "--cacheinfo", &info]);
    let tree = indexed(&["write-tree"]);
    let parent = main_tip(root);
    git(
        root,
        &["commit-tree", &tree, "-p", &parent, "-m", "crafted"],
    )
}

/// An agent-side move: `main` set to `commit` with hooks disabled.
fn agent_move(dir: &Path, commit: &str) {
    let args = ["-c", "core.hooksPath=/dev/null", "update-ref"];
    git(dir, &[&args[..], &["refs/heads/main", commit]].concat());
}

fn assert_target_held(orchestrator: &Orchestrator, accepted: &str, observed: &str) {
    let stage = on_disk(orchestrator);
    assert_eq!(stage.status, StageStatus::MergeBlocked);
    let expected = MergeBlock::TargetHeld {
        target: "main".to_string(),
        accepted: accepted.to_string(),
        observed: observed.to_string(),
    };
    assert_eq!(stage.merge.block, Some(expected));
}

/// Hook installed, stage `s` ready to merge, the guard's baseline recorded,
/// then `main` moved by an agent to a crafted commit adding `name`. Returns
/// the orchestrator, the accepted tip and the moved tip.
fn held_by_crafted_move(root: &Path, name: &str) -> (Orchestrator, String, String) {
    let mut orchestrator = completed_with_work(root);
    assert_attestation_active(root, &orchestrator);
    assert_eq!(guard(&mut orchestrator), None);
    let accepted = main_tip(root);
    let moved = crafted_commit(root, name, "crafted\n");
    agent_move(&worktree(root), &moved);
    (orchestrator, accepted, moved)
}

#[test]
fn unattested_move_holds_the_merge_until_accepted() {
    let repo = hooked_repo();
    let root = repo.path();
    let (mut orchestrator, accepted, moved) = held_by_crafted_move(root, "src/crafted.rs");
    let work_dir = orchestrator.config.work_dir.clone();
    let stage_tip = branch_tip(root);

    assert!(!orchestrator.try_auto_merge(ID));
    assert_target_held(&orchestrator, &accepted, &moved);
    let commits = commit_objects(root);
    let stage_file = stage_file_text(&orchestrator);
    assert_eq!(orchestrator.spawn_merge_resolution_sessions().unwrap(), 0);
    assert_eq!(orchestrator.spawn_merge_resolution_sessions().unwrap(), 0);
    assert_eq!(main_tip(root), moved, "a held target moved");
    assert_eq!(stage_file_text(&orchestrator), stage_file);
    assert_eq!(commit_objects(root), commits, "a held retry wrote a commit");

    accept(root, &work_dir, "main", &moved).unwrap();
    assert_eq!(guard(&mut orchestrator), None);
    orchestrator.spawn_merge_resolution_sessions().unwrap();

    let stage = on_disk(&orchestrator);
    assert!(stage.merged, "the accepted target did not take the merge");
    let parents = git(root, &["rev-list", "--parents", "-n", "1", "main"]);
    let mut parents: Vec<&str> = parents.split_whitespace().skip(1).collect();
    parents.sort_unstable();
    let mut expected = vec![moved.as_str(), stage_tip.as_str()];
    expected.sort_unstable();
    assert_eq!(parents, expected, "main is not a merge of the two tips");
}

#[test]
fn restoring_the_target_releases_the_hold() {
    let repo = hooked_repo();
    let root = repo.path();
    let (mut orchestrator, accepted, moved) = held_by_crafted_move(root, ".claude/settings.json");

    assert!(!orchestrator.try_auto_merge(ID));
    assert_target_held(&orchestrator, &accepted, &moved);

    git(root, &["update-ref", "refs/heads/main", &accepted, &moved]);
    assert_eq!(guard(&mut orchestrator), None);
    orchestrator.spawn_merge_resolution_sessions().unwrap();

    assert!(
        on_disk(&orchestrator).merged,
        "the restored target stayed held"
    );
    let crafted = git_output(root, &["cat-file", "-e", "main:.claude/settings.json"]);
    assert!(!crafted.status.success(), "the crafted file reached main");
}

/// An orchestrator whose graph holds stage `s`, saved Completed with one
/// commit on `loom/s` as its `completed_commit`.
fn graph_orchestrator_with_work(root: &Path) -> Orchestrator {
    let definition = StageDefinition {
        id: ID.into(),
        name: ID.into(),
        working_dir: ".".into(),
        ..Default::default()
    };
    let graph = ExecutionGraph::build(vec![definition]).unwrap();
    let orchestrator = orchestrator_with_graph(root, graph);
    commit_file(&worktree(root), "work.txt", "stage work");
    let stage = Stage {
        id: ID.to_string(),
        status: StageStatus::Completed,
        completed_commit: Some(branch_tip(root)),
        ..Stage::default()
    };
    save_stage(&stage, &orchestrator.config.work_dir).unwrap();
    orchestrator
}

#[test]
fn agent_fast_forward_to_own_branch_is_not_marked_merged() {
    let repo = repo_with_worktree();
    let root = repo.path();
    let mut orchestrator = graph_orchestrator_with_work(root);
    assert_eq!(guard(&mut orchestrator), None);
    let accepted = main_tip(root);
    let own = branch_tip(root);

    agent_move(&worktree(root), &own);
    orchestrator.sync_graph_with_stage_files().unwrap();

    let stage = on_disk(&orchestrator);
    assert!(!stage.merged, "recovery marked a held fast-forward merged");
    assert!(worktree(root).is_dir(), "the stage worktree was cleaned up");

    assert!(!orchestrator.try_auto_merge(ID));
    assert_target_held(&orchestrator, &accepted, &own);
}

#[test]
fn attested_operator_commit_lets_merges_continue() {
    let repo = hooked_repo();
    let root = repo.path();
    let mut orchestrator = completed_with_work(root);
    assert_attestation_active(root, &orchestrator);
    assert_eq!(guard(&mut orchestrator), None);

    commit_file(root, "op.txt", "operator work\n");
    assert_eq!(guard(&mut orchestrator), None);

    assert!(orchestrator.try_auto_merge(ID));
    assert!(on_disk(&orchestrator).merged);
    let work_dir = &orchestrator.config.work_dir;
    assert_eq!(
        accepted_tip(work_dir, "main").unwrap(),
        Some(main_tip(root))
    );
}

#[test]
fn knowledge_only_commit_lets_merges_continue() {
    let repo = hooked_repo();
    let root = repo.path();
    let mut orchestrator = completed_with_work(root);
    assert_attestation_active(root, &orchestrator);
    assert_eq!(guard(&mut orchestrator), None);

    let topic = root.join("doc/loom/knowledge/topic.md");
    std::fs::create_dir_all(topic.parent().unwrap()).unwrap();
    std::fs::write(&topic, "# Topic\n").unwrap();
    git(root, &["add", "doc/loom/knowledge/topic.md"]);
    let commit = [
        "-c",
        "core.hooksPath=/dev/null",
        "commit",
        "-q",
        "-m",
        "knowledge",
    ];
    git(root, &commit);

    assert_eq!(guard(&mut orchestrator), None);
    assert!(orchestrator.try_auto_merge(ID));
    assert!(on_disk(&orchestrator).merged);
}

#[test]
fn restart_keeps_the_hold() {
    let repo = hooked_repo();
    let root = repo.path();
    let (accepted, moved) = {
        let (mut first, accepted, moved) = held_by_crafted_move(root, "src/crafted.rs");
        assert!(guard(&mut first).is_some(), "the move was not held");
        (accepted, moved)
    };

    let mut restarted = orchestrator(root);
    let hold = guard(&mut restarted).expect("the restarted daemon trusted the move");
    assert_eq!(hold.observed, moved);

    assert!(!restarted.try_auto_merge(ID));
    assert_target_held(&restarted, &accepted, &moved);
    let work_dir = &restarted.config.work_dir;
    assert!(recorded_hold(work_dir, "main").unwrap().is_some());
}

#[test]
fn contended_no_worktree_finalize_does_not_settle() {
    let repo = repo_with_worktree();
    let root = repo.path();
    let mut orchestrator = completed_with_work(root);
    let completed = branch_tip(root);
    assert_eq!(guard(&mut orchestrator), None);
    let work_dir = orchestrator.config.work_dir.clone();

    let path = worktree(root);
    git(
        root,
        &["worktree", "remove", "--force", path.to_str().unwrap()],
    );
    git(root, &["branch", "-D", "loom/s"]);
    agent_move(root, &completed);

    let lock = MergeLock::acquire(&work_dir, Duration::from_secs(5)).unwrap();
    let settled = orchestrator.try_auto_merge(ID);
    let merged = on_disk(&orchestrator).merged;
    lock.release().unwrap();

    assert!(
        !settled,
        "the NoWorktree finalize settled an unevaluated move"
    );
    assert!(
        !merged,
        "the stage was marked merged against the live target"
    );
}

#[test]
fn status_alert_names_the_hold() {
    let repo = repo_with_worktree();
    let root = repo.path();
    let mut orchestrator = orchestrator(root);
    assert_eq!(guard(&mut orchestrator), None);
    let moved = crafted_commit(root, ".claude/settings.json", "{}\n");
    agent_move(&worktree(root), &moved);
    assert!(guard(&mut orchestrator).is_some(), "the move was not held");

    let shown = alerts(&orchestrator.config.work_dir, false);

    let named = shown
        .iter()
        .any(|alert| alert.severity == Severity::Warning && alert.text.contains(&moved[..7]));
    assert!(named, "no warning names the held tip: {shown:?}");
}
