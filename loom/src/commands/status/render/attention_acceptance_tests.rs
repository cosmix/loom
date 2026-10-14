//! The acceptance list printed under a blocked stage, the numbers
//! `loom stage amend --index` takes.

use super::*;

fn blocked_with_criteria(criteria: &[&str]) -> StageSummary {
    let mut stage = make_stage_summary("local-runtime", StageStatus::Blocked);
    stage.close_reason = Some("criterion 1 cannot run here".to_string());
    stage.acceptance = criteria
        .iter()
        .map(|command| (*command).to_string())
        .collect();
    stage
}

#[test]
fn a_block_lists_its_criteria_from_zero_after_the_alternatives() {
    let stage = blocked_with_criteria(&["cargo build", "cargo test", "bun run check"]);

    let output = guidance_output(stage, 200);

    let lines: Vec<&str> = output.lines().collect();
    let skip = lines
        .iter()
        .position(|line| line.contains("loom stage skip local-runtime"))
        .unwrap();
    assert!(lines[skip + 1].starts_with("    Acceptance criteria (--index counts from 0):"));
    assert_eq!(
        &lines[skip + 2..],
        [
            "      0  cargo build",
            "      1  cargo test",
            "      2  bun run check"
        ]
    );
}

#[test]
fn indexes_line_up_and_a_long_criterion_wraps_under_its_own_text() {
    let criteria: Vec<String> = (0..11).map(|index| format!("check {index}")).collect();
    let mut stage = blocked_with_criteria(&[]);
    stage.acceptance = criteria;
    stage.acceptance[10] = "cargo test --lib commands::status orchestrator::terminal".to_string();

    let output = guidance_output(stage, 50);

    let listed: Vec<&str> = output
        .lines()
        .skip_while(|line| !line.trim_start().starts_with("Acceptance criteria"))
        .skip(1)
        .collect();
    assert_eq!(listed[0], "       0  check 0");
    assert_eq!(listed[9], "       9  check 9");
    assert_eq!(listed[10], "      10  cargo test --lib commands::status");
    assert_eq!(listed[11], "          orchestrator::terminal");
    for line in &listed {
        assert!(line.chars().count() <= 50, "{line:?}");
    }
}

#[test]
fn a_block_without_criteria_and_a_rejected_review_print_no_list() {
    let none = guidance_output(blocked_with_criteria(&[]), 120);
    assert!(!none.contains("Acceptance criteria"), "{none}");

    let mut rejected = blocked_with_criteria(&["cargo build"]);
    rejected.review_reason = rejected.close_reason.clone();
    let output = guidance_output(rejected, 120);
    assert!(!output.contains("Acceptance criteria"), "{output}");
}

#[test]
fn a_stage_that_is_not_blocked_by_its_agent_prints_no_list() {
    let mut crashed = blocked_with_criteria(&["cargo build"]);
    crashed.close_reason = None;
    crashed.failure_info = Some(FailureInfo {
        failure_type: FailureType::TestFailure,
        detected_at: Utc::now(),
        evidence: vec![],
    });

    let output = guidance_output(crashed, 120);

    assert!(!output.contains("Acceptance criteria"), "{output}");
}
