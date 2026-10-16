//! Focused unit tests for the building blocks of the recovery-path
//! phantom-merge fix (PLAN-fix-phantom-merge.md Fix 1).
//!
//! Full integration coverage of `sync_graph_with_stage_files` requires a
//! live `Orchestrator` with a real `Backend` and `Monitor`, which is too
//! heavy for a unit test. Instead, we exercise the exact helper calls the
//! recovery path makes in sequence:
//!
//! 1. `get_branch_head(&loom/<id>, repo_root)` to derive HEAD when
//!    `completed_commit` is missing.
//! 2. `is_ancestor_of(commit, target, repo_root)` to verify the derived
//!    commit actually landed in the target branch.
//!
//! End-to-end recovery behavior (including the one-shot retry and stuck
//! stage handling) is exercised by `loom/tests/phantom_merge.rs`.
use super::*;
use std::process::Command;
use tempfile::TempDir;

use crate::git::branch::{
    branch_name_for_stage, commits_ahead_of, get_branch_head, is_ancestor_of,
};

#[test]
fn stage_enumeration_and_direct_load_remain_linear_at_scale() {
    for count in [10usize, 100, 1000] {
        let temp = TempDir::new().unwrap();
        for index in 0..count {
            let stage = Stage {
                id: format!("stage-{index}"),
                ..Stage::default()
            };
            let content = crate::verify::transitions::serialize_stage_to_markdown(&stage).unwrap();
            std::fs::write(
                temp.path().join(format!("{index:04}-stage-{index}.md")),
                content,
            )
            .unwrap();
        }

        let mut counter = StageScanCounter::default();
        let paths = scan_stage_paths(temp.path(), &mut counter).unwrap();
        let loaded: Vec<_> = paths
            .iter()
            .map(|path| load_stage_at_path(path).unwrap())
            .collect();

        assert_eq!(loaded.len(), count);
        assert_eq!(counter.directory_reads, 1);
        assert_eq!(counter.entries_visited, count);
    }
}

#[test]
fn recovery_commit_and_merge_reconciliation_preserve_each_other() {
    use std::sync::{Arc, Barrier};

    let temp = TempDir::new().unwrap();
    let work_dir = temp.path().to_path_buf();
    let stage_path = create_merged_recovery_stage(&work_dir);
    let barrier = Arc::new(Barrier::new(3));

    let recovery_dir = work_dir.clone();
    let recovery_path = stage_path.clone();
    let recovery_barrier = Arc::clone(&barrier);
    let recovery = std::thread::spawn(move || {
        recovery_barrier.wait();
        persist_recovery_completed_commit(
            "recovery-merge-race",
            &recovery_path,
            &recovery_dir,
            "verified-commit".to_string(),
        )
        .unwrap();
    });

    let merge_dir = work_dir.clone();
    let merge_barrier = Arc::clone(&barrier);
    let merge = std::thread::spawn(move || {
        merge_barrier.wait();
        crate::orchestrator::merge_attribution::reconcile_attributed_stage_record(
            "recovery-merge-race",
            &merge_dir,
        )
        .unwrap();
    });

    barrier.wait();
    recovery.join().unwrap();
    merge.join().unwrap();

    let stage = crate::verify::transitions::load_stage("recovery-merge-race", &work_dir).unwrap();
    assert_eq!(stage.completed_commit.as_deref(), Some("verified-commit"));
    assert_eq!(stage.status, StageStatus::MergeConflict);
    assert!(stage.merge_conflict);
    assert!(!stage.merged);
}

fn create_merged_recovery_stage(work_dir: &std::path::Path) -> std::path::PathBuf {
    let stage = Stage {
        id: "recovery-merge-race".to_string(),
        name: "Recovery merge race".to_string(),
        status: StageStatus::Completed,
        merged: true,
        ..Stage::default()
    };
    crate::verify::transitions::create_stage(&stage, work_dir).unwrap();
    crate::fs::stage_files::find_stage_file(&work_dir.join("stages"), "recovery-merge-race")
        .unwrap()
        .unwrap()
}

/// A-2 / O-10 regression: `sync_graph_with_stage_files` derives the stage
/// ID from the filename via `crate::fs::stage_files::extract_stage_id`. The
/// previous hand-rolled parser stripped a leading digit then trimmed all
/// leading digits+dashes, corrupting digit-leading IDs (`01-2fa-login.md`
/// → `fa-login`). The stage then never synced and the plan deadlocked.
/// This guards the exact extractor the sync loop now calls.
#[test]
fn extract_stage_id_preserves_digit_leading_ids() {
    use crate::fs::stage_files::extract_stage_id;
    assert_eq!(
        extract_stage_id("01-2fa-login.md"),
        Some("2fa-login".to_string()),
        "digit-leading stage IDs must survive filename parsing during sync"
    );
    assert_eq!(
        extract_stage_id("02-3d-render.md"),
        Some("3d-render".to_string())
    );
    // Non-digit-leading IDs and the no-prefix form must still work.
    assert_eq!(
        extract_stage_id("03-core-arch.md"),
        Some("core-arch".to_string())
    );
    assert_eq!(
        extract_stage_id("plain-id.md"),
        Some("plain-id".to_string())
    );
}

fn init_repo() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(root)
        .output()
        .unwrap();
    std::fs::write(root.join("seed.txt"), "seed").unwrap();
    Command::new("git")
        .args(["add", "seed.txt"])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "seed"])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["branch", "-M", "main"])
        .current_dir(root)
        .output()
        .unwrap();

    tmp
}

/// Fix 1: when `completed_commit` is missing but the loom branch exists,
/// recovery derives HEAD and checks ancestry. If the commit is NOT in the
/// target branch, `merged` must stay false.
///
/// This test stands in for the recovery-path decision: it verifies the
/// helpers produce the exact (branch_head, is_ancestor=false) pair that
/// the production code relies on to REFUSE to set merged=true.
#[test]
fn derive_head_and_ancestry_reports_not_ancestor_when_unmerged() {
    let repo = init_repo();
    let root = repo.path();

    // Create loom/oauth-hardening branch with a commit that stays off main.
    Command::new("git")
        .args(["checkout", "-b", "loom/oauth-hardening"])
        .current_dir(root)
        .output()
        .unwrap();
    std::fs::write(root.join("oauth.rs"), "hardened").unwrap();
    Command::new("git")
        .args(["add", "oauth.rs"])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "hardening"])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["checkout", "main"])
        .current_dir(root)
        .output()
        .unwrap();

    let branch = branch_name_for_stage("oauth-hardening");
    let head = get_branch_head(&branch, root).expect("HEAD derivable");
    assert!(
        !head.is_empty(),
        "branch HEAD should be derivable when branch exists"
    );

    let is_anc = is_ancestor_of(&head, "main", root).expect("ancestry check");
    assert!(
        !is_anc,
        "derived HEAD must NOT be an ancestor of main when branch is unmerged — \
             this is the exact signal that recovery uses to refuse merged=true (Fix 1)"
    );
}

/// Fix 1: when `completed_commit` is missing AND the loom branch is
/// missing, recovery has no way to derive HEAD. The helper must surface
/// the failure so the caller leaves the stage at Completed + !merged.
#[test]
fn derive_head_fails_when_branch_missing() {
    let repo = init_repo();
    let root = repo.path();

    let branch = branch_name_for_stage("nonexistent-stage");
    let result = get_branch_head(&branch, root);
    assert!(
        result.is_err(),
        "branch HEAD derivation must fail when branch does not exist — \
             recovery relies on this to log an error and leave stage as Completed + !merged"
    );
}

/// Fix 1 happy path: if the loom branch exists AND has been merged into
/// main, the ancestry check returns true. The recovery path is allowed
/// to set `merged = true` only in this case.
#[test]
fn ancestry_true_after_branch_merged_into_main() {
    let repo = init_repo();
    let root = repo.path();

    // Create a branch with a commit.
    Command::new("git")
        .args(["checkout", "-b", "loom/landed-stage"])
        .current_dir(root)
        .output()
        .unwrap();
    std::fs::write(root.join("landed.rs"), "done").unwrap();
    Command::new("git")
        .args(["add", "landed.rs"])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "landed"])
        .current_dir(root)
        .output()
        .unwrap();

    // Merge it into main with --no-ff.
    Command::new("git")
        .args(["checkout", "main"])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args([
            "merge",
            "--no-ff",
            "-m",
            "merge landed",
            "loom/landed-stage",
        ])
        .current_dir(root)
        .output()
        .unwrap();

    let branch = branch_name_for_stage("landed-stage");
    let head = get_branch_head(&branch, root).expect("HEAD derivable");
    let is_anc = is_ancestor_of(&head, "main", root).expect("ancestry check");
    assert!(
        is_anc,
        "after branch is merged into main, ancestry must be true — \
             recovery is then allowed to set merged=true"
    );
}

/// Orphan-recovery decision input: a stage whose worktree branch has
/// uncommitted-merged commits beyond `main` should produce a positive
/// `commits_ahead_of` count, which `recover_orphaned_sessions` reads to
/// route the stage to `NeedsHandoff` instead of blindly re-queuing.
///
/// Regression guard: this codifies the exact helper composition the
/// recovery path makes — `branch_name_for_stage` + `commits_ahead_of`
/// against the resolved target — so a refactor that breaks either
/// surface caught here, not in production where the symptom is a
/// wasteful retry of an already-committed stage.
#[test]
fn orphan_with_commits_ahead_signals_handoff_input() {
    let repo = init_repo();
    let root = repo.path();

    create_orphan_branch_with_work(root);

    let with_work = branch_name_for_stage("stage-with-work");
    assert_eq!(
        commits_ahead_of(&with_work, "main", root).unwrap(),
        2,
        "orphan recovery must see commits_ahead > 0 to route to NeedsHandoff"
    );

    // Stage B: branch never had commits (created and abandoned) →
    // no handoff signal, recovery should re-queue.
    Command::new("git")
        .args(["checkout", "-b", "loom/stage-no-work"])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["checkout", "main"])
        .current_dir(root)
        .output()
        .unwrap();

    let no_work = branch_name_for_stage("stage-no-work");
    assert_eq!(
        commits_ahead_of(&no_work, "main", root).unwrap(),
        0,
        "branch without commits must produce no handoff signal so retry can proceed"
    );

    // Stage C: no branch at all → defensive 0, never panics.
    let missing = branch_name_for_stage("never-spawned");
    assert_eq!(
        commits_ahead_of(&missing, "main", root).unwrap(),
        0,
        "missing branch must be treated as zero commits ahead (defensive)"
    );
}

fn create_orphan_branch_with_work(root: &std::path::Path) {
    // Stage A: branch has 2 commits past main → handoff signal.
    Command::new("git")
        .args(["checkout", "-b", "loom/stage-with-work"])
        .current_dir(root)
        .output()
        .unwrap();
    for (i, name) in ["a.rs", "b.rs"].iter().enumerate() {
        std::fs::write(root.join(name), format!("{i}")).unwrap();
        Command::new("git")
            .args(["add", name])
            .current_dir(root)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", &format!("commit-{i}")])
            .current_dir(root)
            .output()
            .unwrap();
    }
    Command::new("git")
        .args(["checkout", "main"])
        .current_dir(root)
        .output()
        .unwrap();
}
