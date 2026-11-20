//! Fixtures for tests that delete an acceptance criterion: a deletion rests
//! on loom's own base run of the criterion and on the `leaves_out` the
//! amendment names.

use std::path::Path;

use crate::kinds::test_support::{self, git_repo};

/// A fresh work directory for a test that applies an amendment, which takes a plan lock;
/// returns from the calling test, naming it, when the lock directory is not writable.
macro_rules! plan_work_dir {
    ($test:literal) => {{
        if loom::process::sandbox_probe::skip_without_plan_locks($test) {
            return;
        }
        tempfile::tempdir().unwrap()
    }};
}
pub(crate) use plan_work_dir;

/// The command of `make_stage_two_criteria`'s acceptance[0].
pub const FIRST_CRITERION: &str = "test -f /__loom_intentionally_wrong/marker.txt";

/// The `plan_patch.leaves_out` of a deletion of acceptance[0]: a criterion
/// can only be deleted when its base run is on record and the amendment names
/// the tests it leaves out and where they run instead.
pub fn leaves_out_first() -> serde_json::Value {
    serde_json::json!([{
        "index": 0,
        "tests": ["marker_is_present"],
        "runs_at": "nowhere",
        "reason": "the criterion's path cannot exist"
    }])
}

/// Record loom's own base run of `stage_id`'s acceptance[0] (`command`) at
/// the stage's base ([`stage_base_of`]): it fails there, counting two failing
/// tests, so a deletion that leaves out one test rests on it.
pub fn seed_failing_base_run(work: &Path, stage_id: &str, command: &str) {
    loom::verify::review::handoffs::save_base_evidence(
        work,
        stage_id,
        &loom::verify::review::handoffs::BaseEvidence {
            index: 0,
            command: command.to_string(),
            base: stage_base_of(work, stage_id),
            passed: false,
            executed: Some(2),
            failed: Some(2),
            build_failed: false,
            timed_out: false,
        },
    )
    .expect("base evidence must save");
}

/// Base runs for `make_stage_three_criteria("s1")`: each accepted verdict
/// deletes whichever criterion is first at that point.
pub fn seed_three_criteria_base_runs(work: &Path) {
    for command in [
        "test -f /__loom_wrong_a/marker.txt",
        "test -f /__loom_wrong_b/marker.txt",
        "ls /tmp",
    ] {
        seed_failing_base_run(work, "s1", command);
    }
}

pub fn verdict_accept_with_amendment() -> serde_json::Value {
    serde_json::json!({
        "verdict": "accept",
        "reasoning": "criterion was overspecified",
        "citations": [
            {"file": "src/a.rs", "line": 1, "excerpt": "X", "claim": "Y"}
        ],
        "plan_patch": {
            "stage_id": "s1",
            "field": "acceptance",
            "patch": {"op": "delete", "index": 0},
            "leaves_out": leaves_out_first(),
            "reason": "test amendment"
        }
    })
}

/// An Accept verdict whose `plan_patch` deletes acceptance[0]. Targets the
/// stage seeded by `make_stage_two_criteria`.
pub fn verdict_accept_delete_first() -> serde_json::Value {
    serde_json::json!({
        "verdict": "accept",
        "reasoning": "acceptance[0] references a path that cannot exist; criterion has no valid interpretation",
        "citations": [
            {
                "file": "PLAN.md",
                "line": 1,
                "excerpt": "intentionally_wrong",
                "claim": "path does not exist in the project root"
            }
        ],
        "plan_patch": {
            "stage_id": "s1",
            "field": "acceptance",
            "patch": {"op": "delete", "index": 0},
            "leaves_out": leaves_out_first(),
            "reason": "criterion path is mechanically wrong"
        }
    })
}

/// The base of stage `stage_id` under the state directory `work`, which the
/// first call makes a repository (`main` with one commit, its checkout
/// identity recorded) where the stage has its worktree `.worktrees/<stage_id>`:
/// a base run counts only at the base the stage has when its narrowing is
/// judged and applied.
fn stage_base_of(work: &Path, stage_id: &str) -> String {
    let root = work.canonicalize().expect("canonical state directory");
    let worktree = root.join(".worktrees").join(stage_id);
    if !root.join(".git").exists() {
        git_repo::TestRepo::builder()
            .branch("main")
            .no_seed()
            .init_in(&root);
        git_repo::commit_file(&root, ".gitignore", "*\n!.gitignore\n", "initial");
        // Loom reads the identity from the state root it resolves for `root`.
        let state = root.join(".loom").join("work");
        test_support::write_main_checkout_identity(&root, &state);
    }
    if !worktree.exists() {
        let branch = format!("loom/{stage_id}");
        let path = worktree.to_str().expect("UTF-8 worktree path");
        git_repo::git(&root, &["worktree", "add", "-q", "-b", &branch, path]);
    }
    git_repo::git(&root, &["rev-parse", "main"])
}
