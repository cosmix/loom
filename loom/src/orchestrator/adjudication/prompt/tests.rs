//! Tests for the adjudication briefing.
//!
//! In their own file so `prompt.rs` stays inside the 400-line ceiling
//! (CLAUDE.md Rule 17); the module path is unchanged.

use super::*;
use crate::plan::schema::AcceptanceCriterion;
use chrono::Utc;

fn stage_with_criteria(criteria: Vec<&str>) -> Stage {
    Stage {
        id: "demo".to_string(),
        name: "Demo".to_string(),
        acceptance: criteria
            .into_iter()
            .map(|s| AcceptanceCriterion::Simple(s.to_string()))
            .collect(),
        ..Default::default()
    }
}

fn dispute(criterion_index: usize) -> DisputeRequest {
    DisputeRequest {
        id: 1,
        stage_id: "demo".to_string(),
        criterion_index,
        reason: "criterion impossible".to_string(),
        evidence_commit: None,
        failure_output: Some("err: something broke".to_string()),
        fix_attempts_at_dispute: 2,
        created_at: Utc::now(),
    }
}

/// `(plan, work_dir, verdict_draft)` for a briefing built in a tmp tree.
fn fixture(
    tmp: &tempfile::TempDir,
) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    let plan = tmp.path().join("PLAN.md");
    std::fs::write(&plan, "stub plan").unwrap();
    let work = tmp.path().join(".loom").join("work");
    std::fs::create_dir_all(&work).unwrap();
    let draft = work.join("disputes/demo/1/verdict.json");
    (plan, work, draft)
}

#[test]
fn build_includes_stage_and_criterion() {
    let tmp = tempfile::tempdir().unwrap();
    let (plan, work, draft) = fixture(&tmp);
    let stage = stage_with_criteria(vec!["cargo test", "cargo clippy"]);
    let p = build(&plan, &stage, &dispute(0), &work, &draft);
    assert!(p.instructions.contains("adjudication session"));
    assert!(p.evidence.contains("Criterion command: `cargo test`"));
    assert!(p.evidence.contains("→ [0]"));
    assert!(p.evidence.contains("err: something broke"));
}

/// The session has no other way to hand its verdict back, so the draft
/// path and the recording command must both survive into the briefing.
#[test]
fn build_tells_the_session_how_to_record_the_verdict() {
    let tmp = tempfile::tempdir().unwrap();
    let (plan, work, draft) = fixture(&tmp);
    let stage = stage_with_criteria(vec!["cargo test"]);
    let p = build(&plan, &stage, &dispute(0), &work, &draft);
    assert!(p.instructions.contains(&draft.display().to_string()));
    assert!(p
        .instructions
        .contains("loom stage adjudicate --stage demo --dispute 1 --verdict-file"));
    assert!(p.render().contains("## Dispute"));
}

/// The agent's account of the criterion is the claim under examination, so
/// the briefing must tell the session to run it — from the worktree root
/// joined with `working_dir`, or a runnable criterion looks broken.
#[test]
fn build_tells_the_session_to_run_the_criterion() {
    let tmp = tempfile::tempdir().unwrap();
    let (plan, work, draft) = fixture(&tmp);
    let exec_dir = tmp.path().join(".worktrees/demo/loom");
    std::fs::create_dir_all(&exec_dir).unwrap();
    let mut stage = stage_with_criteria(vec!["jq -e '[.cities | keys[]] | unique' out.json"]);
    stage.working_dir = Some("loom".to_string());

    let p = build(&plan, &stage, &dispute(0), &work, &draft);

    assert!(p.instructions.contains("RUN THE CRITERION"));
    assert!(
        p.instructions
            .contains(&format!("cd {}", exec_dir.display())),
        "instructions must cd to worktree + working_dir; got:\n{}",
        p.instructions
    );
    assert!(p
        .instructions
        .contains("jq -e '[.cities | keys[]] | unique' out.json"));
    assert!(p.instructions.contains("echo \"exit: $?\""));
    assert!(p.evidence.contains("working_dir: `loom`"));
}

/// A criterion can execute cleanly and still assert a falsehood — a value
/// nobody checked against the source the plan pinned. That is the most common
/// way a criterion is wrong, and it must reach the session as an `accept`: a
/// `reject` ends the autonomous loop, so folding this case into "the tree does
/// not do what the criterion asks" would escalate to a human exactly the
/// dispute this feature exists to settle.
#[test]
fn build_tells_the_session_a_wrong_asserted_value_is_an_accept() {
    let tmp = tempfile::tempdir().unwrap();
    let (plan, work, draft) = fixture(&tmp);
    let stage = stage_with_criteria(vec!["jq -e '.zonesByCode.DE | length == 1' out.json"]);

    let p = build(&plan, &stage, &dispute(0), &work, &draft);

    assert!(
        p.instructions
            .contains("WOULD A CORRECT IMPLEMENTATION PASS THIS"),
        "instructions must pose the discriminating question; got:\n{}",
        p.instructions
    );
    assert!(
        p.instructions
            .contains("the value or condition it asserts is itself wrong"),
        "a criterion that runs and fails on a wrong value needs its own arm; got:\n{}",
        p.instructions
    );
    assert!(
        p.instructions
            .contains("CHECK THAT VALUE against the source the plan pinned"),
        "instructions must send the session to the pinned source; got:\n{}",
        p.instructions
    );
    assert!(
        !p.instructions
            .contains("It fails because the tree does not do what the criterion asks: reject"),
        "no arm may route every failure to reject; got:\n{}",
        p.instructions
    );
}

/// With the worktree gone the criterion would run against a different tree,
/// so the briefing says so instead of quietly substituting the repo root.
#[test]
fn a_missing_worktree_is_flagged_not_pretended_away() {
    let tmp = tempfile::tempdir().unwrap();
    let (plan, work, draft) = fixture(&tmp);
    let stage = stage_with_criteria(vec!["cargo test"]);

    let p = build(&plan, &stage, &dispute(0), &work, &draft);

    assert!(p.instructions.contains("worktree is no longer on disk"));
    assert!(p.evidence.contains("Worktree: gone from disk"));
}

/// A criterion amended away between filing and adjudication leaves nothing
/// to run; the session must be told that rather than shown a blank command.
#[test]
fn a_vanished_criterion_index_is_stated() {
    let tmp = tempfile::tempdir().unwrap();
    let (plan, work, draft) = fixture(&tmp);
    let stage = stage_with_criteria(vec!["cargo test"]);

    let p = build(&plan, &stage, &dispute(7), &work, &draft);

    assert!(p
        .instructions
        .contains("no longer has an acceptance criterion"));
}

#[test]
fn truncation_keeps_total_under_budget() {
    let tmp = tempfile::tempdir().unwrap();
    let plan = tmp.path().join("PLAN.md");
    std::fs::write(&plan, "stub plan".repeat(50_000)).unwrap();
    let work = tmp.path().join(".loom").join("work");
    std::fs::create_dir_all(&work).unwrap();
    let stage = stage_with_criteria(vec!["cargo test"]);
    let mut req = dispute(0);
    req.failure_output = Some("err: ".repeat(50_000));
    let p = build(&plan, &stage, &req, &work, &work.join("verdict.json"));
    assert!(
        p.total_len() <= MAX_PROMPT_BYTES,
        "briefing {} exceeded {}",
        p.total_len(),
        MAX_PROMPT_BYTES,
    );
}
