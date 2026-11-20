use super::*;

#[test]
fn full_briefing_quotes_each_spec_and_its_frozen_file() {
    let fx = contract_fixture();
    freeze(
        &fx.work_dir,
        &[("tests/x_contract.rs", "assert!(false);\n")],
    );
    let input = KindPromptInput {
        stage: &fx.stage,
        dispute_id: 4,
        request: &fx.request,
        site: &fx.site,
        worktree: Some(&fx.worktree),
        work_dir: &fx.work_dir,
    };

    let prompt = build(&input);

    for expected in [
        "## Contract spec",
        "id: rejects-x",
        "file: tests/x_contract.rs",
        "test: tests::rejects_x",
        "cwd: (not set; detected)",
        "recorded cwd: (none recorded)",
        "scenario: x arrives",
        "rejects: accepting x",
        "## Frozen content",
        "assert!(false);",
        "File: `tests/x_contract.rs` (contracts: rejects-x)",
        "## Agent's reason",
        "the new assertion still rejects x",
    ] {
        assert!(prompt.evidence.contains(expected), "missing {expected}");
    }
    for stale in ["## Current content", "## Diff (frozen vs current)"] {
        assert!(!prompt.evidence.contains(stale), "{stale} is gone");
    }
    assert!(prompt
        .instructions
        .contains("loom stage adjudicate --stage demo"));
    assert!(prompt
        .instructions
        .contains("run directory (`loom` in the stage's worktree)"));
}
