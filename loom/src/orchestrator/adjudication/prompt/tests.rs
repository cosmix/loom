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
        kind: DisputeKind::criterion(CriterionField::Acceptance, vec![criterion_index]),
        reason: "criterion impossible".to_string(),
        evidence_commit: None,
        failure_output: Some("err: something broke".to_string()),
        fix_attempts_at_dispute: 2,
        created_at: Utc::now(),
    }
}

/// `(plan, work_dir)` for a briefing built in a tmp tree.
fn fixture(tmp: &tempfile::TempDir) -> (std::path::PathBuf, std::path::PathBuf) {
    let plan = tmp.path().join("PLAN.md");
    std::fs::write(&plan, "stub plan").unwrap();
    let work = tmp.path().join(".loom").join("work");
    std::fs::create_dir_all(&work).unwrap();
    (plan, work)
}

#[test]
fn build_includes_stage_and_criterion() {
    let tmp = tempfile::tempdir().unwrap();
    let (plan, work) = fixture(&tmp);
    let stage = stage_with_criteria(vec!["cargo test", "cargo clippy"]);
    let p = build(&plan, &stage, &dispute(0), &work);
    assert!(p.instructions.contains("adjudication session"));
    assert!(p.evidence.contains("Criterion 0 command: `cargo test`"));
    assert!(p.evidence.contains("→ [0]"));
    assert!(p.evidence.contains("err: something broke"));
}

/// The session has no other way to hand its verdict back, so the draft
/// path and the recording command must both survive into the briefing.
#[test]
fn build_tells_the_session_how_to_record_the_verdict() {
    let tmp = tempfile::tempdir().unwrap();
    let (plan, work) = fixture(&tmp);
    let stage = stage_with_criteria(vec!["cargo test"]);
    let p = build(&plan, &stage, &dispute(0), &work);
    let draft = work.join("disputes/demo/1/verdict.json");
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
    let (plan, work) = fixture(&tmp);
    let exec_dir = tmp.path().join(".worktrees/demo/loom");
    std::fs::create_dir_all(&exec_dir).unwrap();
    let mut stage = stage_with_criteria(vec!["jq -e '[.cities | keys[]] | unique' out.json"]);
    stage.working_dir = Some("loom".to_string());

    let p = build(&plan, &stage, &dispute(0), &work);

    assert!(p.instructions.contains("RUN THE CRITERIA"));
    assert!(p.instructions.contains("loom check demo --no-cache"));
    assert!(p.instructions.contains("# acceptance criterion 0\n"));
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

#[test]
fn a_direct_shell_pass_cannot_reject_a_runner_environment_dispute() {
    let tmp = tempfile::tempdir().unwrap();
    let (plan, work) = fixture(&tmp);
    let stage = stage_with_criteria(vec!["loom knowledge check --strict"]);
    let prompt = build(&plan, &stage, &dispute(0), &work);
    assert!(prompt
        .instructions
        .contains("A direct-shell pass alone cannot\n  justify rejection"));
    assert!(prompt
        .instructions
        .contains("portable amendment preserves the assertion"));
    assert!(prompt
        .instructions
        .contains("the system applies it automatically"));
    assert!(!prompt.instructions.contains("It PASSES (exit 0)"));
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
    let (plan, work) = fixture(&tmp);
    let stage = stage_with_criteria(vec!["jq -e '.zonesByCode.DE | length == 1' out.json"]);

    let p = build(&plan, &stage, &dispute(0), &work);

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
    let (plan, work) = fixture(&tmp);
    let stage = stage_with_criteria(vec!["cargo test"]);

    let p = build(&plan, &stage, &dispute(0), &work);

    assert!(p.instructions.contains("worktree is no longer on disk"));
    assert!(p.evidence.contains("Worktree: gone from disk"));
}

/// A criterion amended away between filing and adjudication leaves nothing
/// to run; the session must be told that rather than shown a blank command.
#[test]
fn a_vanished_criterion_index_is_stated() {
    let tmp = tempfile::tempdir().unwrap();
    let (plan, work) = fixture(&tmp);
    let stage = stage_with_criteria(vec!["cargo test"]);

    let p = build(&plan, &stage, &dispute(7), &work);

    assert!(p
        .instructions
        .contains("no longer has an acceptance criterion"));
}

/// One dispute over criteria 13 and 14 marks both, runs both, and sends the
/// session to every other failing criterion too: a criterion it never judged
/// was once taken by the next session as reviewed and rejected.
#[test]
fn a_dispute_over_two_criteria_judges_both_and_every_other_failure() {
    let tmp = tempfile::tempdir().unwrap();
    let (plan, work) = fixture(&tmp);
    let commands: Vec<String> = (0..15).map(|i| format!("cargo test case_{i}")).collect();
    let stage = stage_with_criteria(commands.iter().map(String::as_str).collect());
    let mut request = dispute(13);
    request.kind = DisputeKind::criterion(CriterionField::Acceptance, vec![13, 14]);

    let p = build(&plan, &stage, &request, &work);

    assert!(p
        .instructions
        .contains("Judge every disputed criterion AND every other criterion `loom check` reports\nfailing; a verdict may patch any of them."));
    assert!(p
        .instructions
        .contains("disputed acceptance criteria\n13, 14."));
    assert!(p.instructions.contains("cargo test case_13\necho"));
    assert!(p.instructions.contains("cargo test case_14\necho"));
    assert!(p.instructions.contains("\"patches\": ["));
    assert!(p.evidence.contains("Criterion indices: 13, 14\n"));
    assert!(p.evidence.contains("→ [13] cargo test case_13"));
    assert!(p.evidence.contains("→ [14] cargo test case_14"));
    assert!(p.evidence.contains("  [12] cargo test case_12"));
}

/// A criterion that fails only where completion runs ends in a verdict, and
/// evidence nobody can produce is never asked for.
#[test]
fn a_runner_only_failure_is_amended_or_deleted_never_left_for_evidence() {
    let tmp = tempfile::tempdir().unwrap();
    let (plan, work) = fixture(&tmp);
    let stage = stage_with_criteria(vec!["curl -sf https://example.com"]);

    let p = build(&plan, &stage, &dispute(0), &work);

    for rule in [
        "only because of where completion runs",
        "`timeout_secs`, 1-3600",
        "what verification the deletion leaves to\n  integration-verify",
        "Never request evidence no agent can produce",
        "A need only a person can meet",
    ] {
        assert!(p.instructions.contains(rule), "missing {rule:?}");
    }
    assert!(!p
        .instructions
        .contains("request the missing runner evidence"));
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
    let p = build(&plan, &stage, &req, &work);
    assert!(
        p.total_len() <= MAX_PROMPT_BYTES,
        "briefing {} exceeded {}",
        p.total_len(),
        MAX_PROMPT_BYTES,
    );
}
