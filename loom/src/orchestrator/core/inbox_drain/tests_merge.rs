//! `merge-resolved` end to end: the daemon's ancestry proof decides, the
//! worktree cleanup runs through `MergeLifecycle`, and the request is refused
//! for any other session kind or stage.

use std::path::Path;
use std::process::Command;

use chrono::Utc;

use crate::fs::inbox::LedgerOutcome;
use crate::models::session::{SessionStatus, SessionType};
use crate::models::stage::StageStatus;
use crate::orchestrator::core::{Orchestrator, OrchestratorConfig};
use crate::plan::ExecutionGraph;
use crate::relay::RequestKind;
use crate::verify::transitions::load_stage;

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

/// A repository on `main` with the stage branch `loom/s1` one commit ahead,
/// merged into `main` only when `merged`.
fn repository(fx: &Fixture, merged: bool) {
    let root = &fx.repo_root;
    git(root, &["init", "-q", "-b", "main"]);
    std::fs::write(root.join(".gitignore"), ".loom/\n").unwrap();
    std::fs::write(root.join("a.txt"), "a\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "base"]);
    git(root, &["checkout", "-q", "-b", "loom/s1"]);
    std::fs::write(root.join("b.txt"), "b\n").unwrap();
    git(root, &["add", "b.txt"]);
    git(root, &["commit", "-q", "-m", "stage work"]);
    git(root, &["checkout", "-q", "main"]);
    if merged {
        git(
            root,
            &["merge", "-q", "--no-ff", "loom/s1", "-m", "merge stage"],
        );
    }
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

#[test]
fn merge_resolved_with_ancestry_proof_completes_the_stage_and_removes_its_branch() {
    let fx = fixture();
    repository(&fx, true);
    fx.stage(StageStatus::MergeConflict, None);
    let mut orchestrator = orchestrator(&fx);

    assert_eq!(
        resolve(&fx, &mut orchestrator),
        Some(LedgerOutcome::Applied)
    );

    let stage = load_stage(STAGE, &fx.work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::Completed);
    assert!(stage.merged);
    assert!(git(&fx.repo_root, &["branch", "--list", "loom/s1"]).is_empty());
}

#[test]
fn merge_resolved_without_ancestry_proof_leaves_merged_false() {
    let fx = fixture();
    repository(&fx, false);
    fx.stage(StageStatus::MergeConflict, None);
    let mut orchestrator = orchestrator(&fx);

    assert_eq!(
        resolve(&fx, &mut orchestrator),
        Some(LedgerOutcome::Refused)
    );

    let stage = load_stage(STAGE, &fx.work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::MergeConflict);
    assert!(!stage.merged);
    assert_eq!(
        git(&fx.repo_root, &["branch", "--list", "loom/s1"]),
        "loom/s1"
    );
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
