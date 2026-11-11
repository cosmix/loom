//! The review gate across a merge-base move: a merge of the target branch
//! keeps a round covering the stage only while the stage's changes are the
//! reviewed ones.

use super::*;
use crate::git::target_guard::test_support::{commit_file, git};
use crate::models::stage::StageStatus;
use crate::verify::contracts::test_support::{contract_worktree, pinned};
use crate::verify::review::store::{write_round, RECORD_VERSION};
use crate::verify::transitions::save_stage;
use std::path::PathBuf;
use tempfile::TempDir;

const STAGE: &str = "s1";

struct Fixture {
    _temp: TempDir,
    project: PathBuf,
    work_dir: PathBuf,
    worktree: PathBuf,
    stage: Stage,
}

/// A project on `main` whose v2 standard stage `s1` committed `files` in its
/// worktree, with a round recorded at that tree.
fn reviewed(files: &[(&str, &str)]) -> Fixture {
    let fx = committed(files);
    record(&fx);
    fx
}

fn record(fx: &Fixture) {
    write_round(&fx.work_dir, STAGE, &round_at(&observe(fx))).unwrap();
}

/// [`reviewed`] without the round.
fn committed(files: &[(&str, &str)]) -> Fixture {
    let temp = TempDir::new().unwrap();
    let project = temp.path().join("repo");
    let worktree = contract_worktree(&project, STAGE);
    let work_dir = project.join(".loom").join("work");
    std::fs::create_dir_all(work_dir.join("stages")).unwrap();
    let stage = Stage {
        id: STAGE.to_string(),
        status: StageStatus::Executing,
        plan_version: 2,
        stage_type: StageType::Standard,
        worktree: Some(STAGE.to_string()),
        ..Stage::default()
    };
    save_stage(&stage, &work_dir).unwrap();
    for (path, text) in files {
        commit_file(&worktree, path, text);
    }
    Fixture {
        _temp: temp,
        project,
        work_dir,
        worktree,
        stage,
    }
}

fn observe(fx: &Fixture) -> ChangeFingerprint {
    fingerprint::compute_local(&pinned(&fx.worktree), "main").unwrap()
}

fn round_at(seen: &ChangeFingerprint) -> ReviewRound {
    ReviewRound {
        version: RECORD_VERSION,
        round: 1,
        agent_id: "agent-1".to_string(),
        harvested_at: chrono::Utc::now(),
        fingerprint: seen.value.clone(),
        base: seen.base.clone(),
        behaviour: seen.behaviour.clone(),
        files: seen.files.clone(),
        malformed: None,
        findings: Vec::new(),
        resolved: Vec::new(),
        unresolved: Vec::new(),
        suggestion_memory_ids: Vec::new(),
    }
}

/// Commit `files` on `main`, then merge `main` into the stage with `strategy`
/// options (`-X ours`, `-X theirs`) settling any conflict.
fn merge_target(fx: &Fixture, files: &[(&str, &str)], strategy: &[&str]) {
    for (path, text) in files {
        commit_file(&fx.project, path, text);
    }
    let mut args = vec!["merge", "-q", "--no-edit"];
    args.extend(strategy);
    args.push("main");
    git(&fx.worktree, &args);
}

/// The gate's verdict on the stage's current tree, after asserting that the
/// merge did move the base.
fn gate(fx: &Fixture) -> Result<()> {
    let current = observe(fx);
    let latest = &store::load_rounds(&fx.work_dir, STAGE).unwrap()[0];
    assert_ne!(latest.base, current.base, "the merge must move the base");
    check(&fx.stage, &fx.work_dir, &current)
}

fn stale(fx: &Fixture) -> String {
    format!("{:#}", gate(fx).unwrap_err())
}

#[test]
fn a_base_move_with_identical_stage_files_stays_covered() {
    let fx = reviewed(&[("src/stage.rs", "fn stage() {}\n")]);
    merge_target(&fx, &[("src/other.rs", "fn other() {}\n")], &[]);
    gate(&fx).unwrap();
}

#[test]
fn a_stage_file_changed_after_a_base_move_stays_stale() {
    let fx = reviewed(&[("src/stage.rs", "fn stage() {}\n")]);
    merge_target(&fx, &[("src/other.rs", "fn other() {}\n")], &[]);
    std::fs::write(fx.worktree.join("src/stage.rs"), "fn changed() {}\n").unwrap();
    let message = stale(&fx);
    assert!(message.contains("the base commit moved from"), "{message}");
    assert!(message.contains("changed since: src/stage.rs"), "{message}");
}

/// The post-mortem shape: both branches rebuilt the bundle, and the merge
/// rebuilt it again from the combined sources. Nothing the gate reads proves
/// the rebuilt bundle equals a build of reviewed sources.
#[test]
fn a_generated_file_differing_after_a_base_move_stays_stale() {
    let fx = reviewed(&[
        ("web/src/a.ts", "export const a = 1;\n"),
        ("web/dist/index.js", "bundle(a)\n"),
    ]);
    merge_target(
        &fx,
        &[
            ("web/src/b.ts", "export const b = 2;\n"),
            ("web/dist/index.js", "bundle(b)\n"),
        ],
        &["-X", "ours"],
    );
    std::fs::write(fx.worktree.join("web/dist/index.js"), "bundle(a, b)\n").unwrap();
    let message = stale(&fx);
    assert!(
        message.contains("changed since: web/dist/index.js"),
        "{message}"
    );
    assert!(!message.contains("web/src/a.ts"), "{message}");
}

/// A reviewed change the target branch now carries byte for byte, a deletion
/// among them, leaves the stage's change set and stays covered.
#[test]
fn a_reviewed_change_the_target_adopted_stays_covered() {
    let fx = committed(&[
        ("src/stage.rs", "fn stage() {}\n"),
        ("src/fix.rs", "fn fix() {}\n"),
    ]);
    git(&fx.worktree, &["rm", "-q", "README.md"]);
    git(&fx.worktree, &["commit", "-q", "-m", "drop readme"]);
    record(&fx);
    git(&fx.project, &["rm", "-q", "README.md"]);
    git(&fx.project, &["commit", "-q", "-m", "drop readme"]);
    merge_target(&fx, &[("src/fix.rs", "fn fix() {}\n")], &[]);
    assert!(!observe(&fx).files.contains_key("src/fix.rs"));
    gate(&fx).unwrap();
}

/// A conflict settled for the target's version drops the reviewed change.
#[test]
fn a_reviewed_change_dropped_for_the_targets_version_is_stale() {
    let fx = reviewed(&[
        ("src/stage.rs", "fn stage() {}\n"),
        ("src/shared.rs", "fn reviewed() {}\n"),
    ]);
    merge_target(
        &fx,
        &[("src/shared.rs", "fn target() {}\n")],
        &["-X", "theirs"],
    );
    let message = stale(&fx);
    assert!(
        message.contains("changed since: src/shared.rs"),
        "{message}"
    );
}

/// A conflict settled for the stage's version keeps the reviewed bytes but
/// would revert the target's change to the same file.
#[test]
fn reviewed_bytes_kept_over_a_target_change_are_stale() {
    let fx = reviewed(&[
        ("src/stage.rs", "fn stage() {}\n"),
        ("src/shared.rs", "fn reviewed() {}\n"),
    ]);
    merge_target(
        &fx,
        &[("src/shared.rs", "fn target() {}\n")],
        &["-X", "ours"],
    );
    let message = stale(&fx);
    assert!(
        message.contains("also changed on the target branch: src/shared.rs"),
        "{message}"
    );
    assert!(!message.contains("changed since:"), "{message}");
}

/// The base-move rules read the round's per-path digests, so a round whose
/// digests are not the ones its fingerprint hashed proves nothing.
#[test]
fn a_round_whose_files_do_not_hash_to_its_fingerprint_is_stale() {
    let fx = committed(&[("src/stage.rs", "fn stage() {}\n")]);
    let reviewed = observe(&fx);
    merge_target(&fx, &[("src/other.rs", "fn other() {}\n")], &[]);
    std::fs::write(fx.worktree.join("src/stage.rs"), "fn unreviewed() {}\n").unwrap();
    let mut forged = round_at(&reviewed);
    forged.files = observe(&fx).files;
    write_round(&fx.work_dir, STAGE, &forged).unwrap();
    let message = stale(&fx);
    assert!(message.contains("could not be compared"), "{message}");
    assert!(
        message.contains("do not hash to its fingerprint"),
        "{message}"
    );
}

/// A recorded base that is no object id never reaches git, and fails closed.
#[test]
fn a_base_that_is_no_object_id_is_stale() {
    let fx = reviewed(&[("src/stage.rs", "fn stage() {}\n")]);
    let mut current = observe(&fx);
    current.base = "--output=/tmp/x".to_string();
    current.value = "sha256:other".to_string();
    let message = format!(
        "{:#}",
        check(&fx.stage, &fx.work_dir, &current).unwrap_err()
    );
    assert!(message.contains("could not be compared"), "{message}");
    assert!(message.contains("no full object id"), "{message}");
}
