//! `merge-resolved` end to end: the daemon checks the stage worktree, lands
//! the merge, and its ancestry proof decides; the worktree stays for the
//! resolver's exit to remove, and the request is refused for any other
//! session kind or stage.

use std::path::Path;
use std::process::Command;

use chrono::Utc;

use crate::fs::inbox::LedgerOutcome;
use crate::models::session::{SessionStatus, SessionType};
use crate::models::stage::StageStatus;
use crate::orchestrator::core::{Orchestrator, OrchestratorConfig};
use crate::plan::ExecutionGraph;
use crate::relay::RequestKind;
use crate::verify::transitions::{load_stage, update_stage};

use super::test_support::{entry_for, fixture, payload_for, Fixture, STAGE};
use super::{run_pass, Tick};

/// Run `git` with ambient global and system config shut out, for this child
/// process only.
fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", root.join(".no-global-config"))
        .env("GIT_CONFIG_SYSTEM", root.join(".no-system-config"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "loom-test")
        .env("GIT_AUTHOR_EMAIL", "loom-test@example.com")
        .env("GIT_COMMITTER_NAME", "loom-test")
        .env("GIT_COMMITTER_EMAIL", "loom-test@example.com")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// A repository on `main` with the stage worktree `.worktrees/s1` on `loom/s1`,
/// one commit ahead. The identity is set in the repository's own config,
/// because the daemon's merge runs git without this module's environment.
fn repository(fx: &Fixture) {
    let root = &fx.repo_root;
    git(root, &["init", "-q", "-b", "main"]);
    git(root, &["config", "user.name", "loom-test"]);
    git(root, &["config", "user.email", "loom-test@example.com"]);
    std::fs::write(root.join(".gitignore"), ".loom/\n.worktrees/\n").unwrap();
    std::fs::write(root.join("a.txt"), "a\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "base"]);
    let worktree = worktree_of(fx);
    git(
        root,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "loom/s1",
            worktree.to_str().unwrap(),
        ],
    );
    std::fs::write(worktree.join("b.txt"), "b\n").unwrap();
    git(&worktree, &["add", "b.txt"]);
    git(&worktree, &["commit", "-q", "-m", "stage work"]);
}

fn worktree_of(fx: &Fixture) -> std::path::PathBuf {
    fx.repo_root.join(".worktrees").join(STAGE)
}

/// Commit `name` on `main`, which the stage worktree then lacks.
fn advance_main(fx: &Fixture, name: &str, text: &str) {
    std::fs::write(fx.repo_root.join(name), text).unwrap();
    git(&fx.repo_root, &["add", name]);
    git(&fx.repo_root, &["commit", "-q", "-m", "main work"]);
}

/// An orchestrator over the fixture, on the tmux lane so that building it
/// never probes the host for a terminal emulator.
fn orchestrator(fx: &Fixture) -> Orchestrator {
    std::fs::write(
        fx.work_dir.join("config.toml"),
        "[terminal]\nbackend = \"tmux\"\n",
    )
    .unwrap();
    let config = OrchestratorConfig {
        work_dir: fx.work_dir.clone(),
        repo_root: fx.repo_root.clone(),
        base_branch: Some("main".to_string()),
        enable_skill_routing: false,
        ..Default::default()
    };
    Orchestrator::new(config, ExecutionGraph::build(Vec::new()).unwrap()).unwrap()
}

/// A Merge session relays `merge-resolved`; one pass settles it.
fn resolve(fx: &Fixture, orchestrator: &mut Orchestrator) -> Option<LedgerOutcome> {
    let record = fx.record(SessionType::Merge, SessionStatus::Running);
    let kind = RequestKind::MergeResolved;
    let entry = fx.relay(&record, kind, payload_for(kind));
    let tick = Tick {
        scratch_root: None,
        now: Utc::now(),
    };
    run_pass(orchestrator, &tick);
    fx.outcome(&record.id, &entry.id)
}

/// The stage in `MergeConflict` with `completed_commit` recorded as given.
fn conflict_stage(fx: &Fixture, completed_commit: Option<&str>) {
    fx.stage(StageStatus::MergeConflict, None);
    update_stage(STAGE, &fx.work_dir, |stage| {
        stage.completed_commit = completed_commit.map(str::to_string);
        stage.acceptance = vec![crate::models::stage::AcceptanceCriterion::Simple(
            "true".into(),
        )];
        Ok(())
    })
    .unwrap();
}

/// The stage as the daemon meets it after a resolver merged the (moved) target
/// into the worktree: `main` advanced, and the worktree merged it.
fn resolved_stage(fx: &Fixture) -> Orchestrator {
    repository(fx);
    let own_commit = git(&worktree_of(fx), &["rev-parse", "HEAD"]);
    advance_main(fx, "m.txt", "m\n");
    git(
        &worktree_of(fx),
        &["merge", "-q", "main", "-m", "merge main"],
    );
    conflict_stage(fx, Some(&own_commit));
    orchestrator(fx)
}

/// A refused `--resolved`: the stage keeps `MergeConflict`, `merged` stays
/// false, `main` does not move, and the worktree stays.
fn assert_refused(fx: &Fixture, orchestrator: &mut Orchestrator) {
    let main_before = git(&fx.repo_root, &["rev-parse", "main"]);
    assert_eq!(resolve(fx, orchestrator), Some(LedgerOutcome::Refused));
    let stage = load_stage(STAGE, &fx.work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::MergeConflict);
    assert!(!stage.merged);
    assert_eq!(git(&fx.repo_root, &["rev-parse", "main"]), main_before);
    assert!(worktree_of(fx).is_dir());
}

#[test]
fn merge_resolved_lands_the_merge_and_leaves_the_worktree_for_the_resolver_exit() {
    let fx = fixture();
    let mut orchestrator = resolved_stage(&fx);

    assert_eq!(
        resolve(&fx, &mut orchestrator),
        Some(LedgerOutcome::Applied)
    );

    let stage = load_stage(STAGE, &fx.work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::Completed);
    assert!(stage.merged);
    assert_eq!(
        stage.completed_commit,
        Some(git(&worktree_of(&fx), &["rev-parse", "HEAD"]))
    );
    assert_eq!(
        git(&fx.repo_root, &["cat-file", "-t", "main:b.txt"]),
        "blob"
    );
    assert!(worktree_of(&fx).is_dir(), "the resolver still runs there");
    git(
        &fx.repo_root,
        &["rev-parse", "--verify", "refs/heads/loom/s1"],
    );
}

#[test]
fn merge_resolved_lands_when_the_target_moved_after_the_resolution() {
    let fx = fixture();
    let mut orchestrator = resolved_stage(&fx);
    advance_main(&fx, "later.txt", "later\n");

    assert_eq!(
        resolve(&fx, &mut orchestrator),
        Some(LedgerOutcome::Applied)
    );

    let stage = load_stage(STAGE, &fx.work_dir).unwrap();
    assert!(stage.merged);
    assert_eq!(
        git(&fx.repo_root, &["cat-file", "-t", "main:later.txt"]),
        "blob"
    );
}

#[test]
fn merge_resolved_is_refused_when_the_resolver_dropped_the_stage_commit() {
    let fx = fixture();
    let mut orchestrator = resolved_stage(&fx);
    git(&worktree_of(&fx), &["reset", "-q", "--hard", "main"]);
    assert_refused(&fx, &mut orchestrator);
}

#[test]
fn merge_resolved_is_refused_without_a_recorded_completed_commit() {
    let fx = fixture();
    repository(&fx);
    conflict_stage(&fx, None);
    let mut orchestrator = orchestrator(&fx);
    assert_refused(&fx, &mut orchestrator);
}

#[test]
fn merge_resolved_is_refused_with_a_merge_in_progress_or_unmerged_paths() {
    let fx = fixture();
    repository(&fx);
    let worktree = worktree_of(&fx);
    std::fs::write(worktree.join("a.txt"), "stage side\n").unwrap();
    git(&worktree, &["commit", "-q", "-am", "stage edit"]);
    let own_commit = git(&worktree, &["rev-parse", "HEAD"]);
    advance_main(&fx, "a.txt", "main side\n");
    let merge = Command::new("git")
        .args(["merge", "main"])
        .current_dir(&worktree)
        .output()
        .unwrap();
    assert!(!merge.status.success(), "the merge must conflict");
    conflict_stage(&fx, Some(&own_commit));
    let mut orchestrator = orchestrator(&fx);
    assert_refused(&fx, &mut orchestrator);
}

#[test]
fn merge_resolved_is_refused_with_an_uncommitted_tracked_change() {
    let fx = fixture();
    let mut orchestrator = resolved_stage(&fx);
    std::fs::write(worktree_of(&fx).join("b.txt"), "edited\n").unwrap();
    assert_refused(&fx, &mut orchestrator);
}

#[test]
fn merge_resolved_is_refused_for_another_session_kind_or_stage() {
    let fx = fixture();
    fx.stage(StageStatus::MergeConflict, None);
    let kind = RequestKind::MergeResolved;
    let stage_session = fx.record(SessionType::Stage, SessionStatus::Running);
    let from_stage_session = fx.relay(&stage_session, kind, payload_for(kind));
    let merge_session = fx.record(SessionType::Merge, SessionStatus::Running);
    let mut other_stage = entry_for(&merge_session, kind, payload_for(kind));
    other_stage.stage_id = "other-stage".to_string();
    fx.plant(
        &merge_session.id,
        &format!("{}.json", other_stage.id),
        &other_stage.encode(),
    );
    let mut host = fx.host(true);

    run_pass(&mut host, &fx.tick(Utc::now()));

    assert!(host.merges.is_empty());
    assert_eq!(
        fx.outcome(&stage_session.id, &from_stage_session.id),
        Some(LedgerOutcome::Refused)
    );
    assert_eq!(
        fx.outcome(&merge_session.id, &other_stage.id),
        Some(LedgerOutcome::Refused)
    );
}

#[test]
fn merge_resolved_refuses_a_branch_that_touches_a_control_path() {
    let fx = fixture();
    let mut orchestrator = resolved_stage(&fx);
    let worktree = worktree_of(&fx);
    std::fs::create_dir_all(worktree.join(".claude")).unwrap();
    std::fs::write(worktree.join(".claude/settings.json"), "{}\n").unwrap();
    git(&worktree, &["add", ".claude/settings.json"]);
    git(&worktree, &["commit", "-q", "-m", "resolver edit"]);
    let main_before = git(&fx.repo_root, &["rev-parse", "main"]);

    assert_eq!(
        resolve(&fx, &mut orchestrator),
        Some(LedgerOutcome::Refused)
    );

    let stage = load_stage(STAGE, &fx.work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::NeedsHumanReview);
    assert!(!stage.merged);
    assert!(stage
        .review_reason
        .unwrap()
        .contains(".claude/settings.json"));
    assert_eq!(git(&fx.repo_root, &["rev-parse", "main"]), main_before);
}

mod settle_for_landing {
    use super::super::merge_resolved::settle_for_landing;
    use super::super::Settle;
    use crate::git::merge::MergeBlock;
    use crate::orchestrator::core::merge_handler::Landing;

    fn settle(landing: Landing) -> Settle {
        settle_for_landing(landing, "stage-a", "main")
    }

    #[test]
    fn merged_applies_with_target() {
        match settle(Landing::Merged) {
            Settle::Applied(Some(note)) => assert!(note.contains("merged into 'main'")),
            _ => panic!("expected Applied with a note"),
        }
    }

    #[test]
    fn held_refuses_with_control_paths() {
        match settle(Landing::Held) {
            Settle::Refused(reason) => assert!(reason.contains("human review")),
            _ => panic!("expected Refused"),
        }
    }

    #[test]
    fn conflict_refuses_with_paths_and_rerun() {
        let landing = Landing::Conflict(vec!["a.rs".into(), "b.rs".into()]);
        match settle(landing) {
            Settle::Refused(reason) => {
                assert!(reason.contains("a.rs, b.rs"));
                assert!(reason.contains("rerun --resolved"));
            }
            _ => panic!("expected Refused"),
        }
    }

    #[test]
    fn blocked_applies_with_block_text() {
        match settle(Landing::Blocked(MergeBlock::TargetMoved)) {
            Settle::Applied(Some(note)) => assert!(note.contains("the merge is blocked")),
            _ => panic!("expected Applied with a note"),
        }
    }

    #[test]
    fn unproven_refuses_naming_stage_and_target() {
        match settle(Landing::Unproven) {
            Settle::Refused(reason) => {
                assert!(reason.contains("stage-a") && reason.contains("'main'"));
            }
            _ => panic!("expected Refused"),
        }
    }

    #[test]
    fn failed_refuses_with_error_text() {
        match settle(Landing::Failed("boom".into())) {
            Settle::Refused(reason) => assert_eq!(reason, "boom"),
            _ => panic!("expected Refused"),
        }
    }
}

#[test]
fn merge_resolved_refuses_failing_completion_without_replacing_original_proof() {
    let fx = fixture();
    let mut orchestrator = resolved_stage(&fx);
    update_stage(STAGE, &fx.work_dir, |stage| {
        stage.acceptance = vec![crate::models::stage::AcceptanceCriterion::Simple(
            "false".into(),
        )];
        Ok(())
    })
    .unwrap();
    let original = load_stage(STAGE, &fx.work_dir).unwrap().completed_commit;
    let main = git(&fx.repo_root, &["rev-parse", "main"]);
    assert_eq!(
        resolve(&fx, &mut orchestrator),
        Some(LedgerOutcome::Refused)
    );
    let stage = load_stage(STAGE, &fx.work_dir).unwrap();
    assert!(!stage.merged);
    assert_eq!(stage.completed_commit, original);
    assert_eq!(git(&fx.repo_root, &["rev-parse", "main"]), main);
}
