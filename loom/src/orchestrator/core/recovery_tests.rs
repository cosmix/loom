//! Stage-store locking and Git ancestry primitives used by recovery.
//! Ancestry alone never supplies missing completion proof. Full daemon
//! recovery coverage also lives in merge-handler and phantom-merge tests.
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
fn verified_commit_and_merge_reconciliation_preserve_each_other() {
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
        update_stage_at_path(
            "recovery-merge-race",
            &recovery_path,
            &recovery_dir,
            |stage| {
                stage.completed_commit = Some("verified-commit".to_string());
                Ok(())
            },
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

/// A branch tip outside the target cannot establish landed ancestry.
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
        "branch HEAD must not be an ancestor of main when the branch is unmerged"
    );
}

/// Looking up a missing branch must report failure.
#[test]
fn derive_head_fails_when_branch_missing() {
    let repo = init_repo();
    let root = repo.path();

    let branch = branch_name_for_stage("nonexistent-stage");
    let result = get_branch_head(&branch, root);
    assert!(
        result.is_err(),
        "branch HEAD lookup must fail when the branch does not exist"
    );
}

/// A merged branch supplies ancestry evidence, independently of completion proof.
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
        "after the branch is merged into main, ancestry must be true"
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
