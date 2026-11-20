//! The contract briefing of a dispute that carries a replacement for the
//! frozen file: it is reviewed by reading, never presented as a run result.

use super::tests::{contract_fixture, freeze, input_of, ContractFixture};
use super::*;
use crate::models::dispute::FileReplacement;

const FROZEN: &str = "#[test]\nfn rejects_x() { assert!(rejects(\"x\")); }\n";
/// Does not compile: judging it by running anything would be a mistake.
const BROKEN_REPLACEMENT: &str = "#[test]\nfn rejects_x( { assert!(;\n";

/// A fixture whose frozen test passes and whose worktree file is unmodified,
/// disputing with `replacement`.
fn fixture_with(replacement: Option<&str>) -> ContractFixture {
    let mut fx = contract_fixture();
    freeze(&fx.work_dir, &[("tests/x_contract.rs", FROZEN)]);
    fs::write(fx.site.path.join("tests/x_contract.rs"), FROZEN).unwrap();
    fx.request.kind = DisputeKind::Contract {
        contract_ids: vec!["rejects-x".to_string()],
        replacements: replacement
            .map(|content| FileReplacement {
                file: "tests/x_contract.rs".to_string(),
                content: content.to_string(),
                base_sha256: None,
            })
            .into_iter()
            .collect(),
    };
    fx
}

fn headings(evidence: &str) -> Vec<&str> {
    evidence
        .lines()
        .filter(|line| line.starts_with("## "))
        .collect()
}

#[test]
fn the_replacement_is_quoted_and_diffed_against_the_frozen_file() {
    let fx = fixture_with(Some(BROKEN_REPLACEMENT));

    let prompt = build(&input_of(&fx));

    assert!(prompt.evidence.contains(&format!(
        "## Proposed replacement\n\nFile: `tests/x_contract.rs` (contracts: rejects-x)\n\n\
         ```text\n{BROKEN_REPLACEMENT}```\n"
    )));
    let diff_at = prompt
        .evidence
        .find("## Diff (frozen vs replacement)")
        .expect("a replacement diff section");
    let diff = &prompt.evidence[diff_at..];
    assert!(
        diff.contains("-#[test]\n") || diff.contains("-fn rejects_x() {"),
        "{diff}"
    );
    assert!(diff.contains("+fn rejects_x( { assert!(;"), "{diff}");
}

#[test]
fn without_a_replacement_the_briefing_has_no_replacement_parts() {
    let fx = fixture_with(None);

    let prompt = build(&input_of(&fx));

    assert_eq!(
        headings(&prompt.evidence),
        [
            "## Dispute",
            "## Contract spec",
            "## Frozen content",
            "## Agent's reason",
            "## Other frozen files changed since the freeze",
        ]
    );
    for absent in [
        "replacement has not been run",
        "proposed replacement",
        "writes the replacement into the worktree",
    ] {
        assert!(!prompt.instructions.contains(absent), "{absent}");
    }
    assert!(prompt
        .instructions
        .contains("contract, frozen file, or diff. A citation has file, optional"));
}

#[test]
fn every_contract_prompt_forbids_asking_for_an_edit_of_a_frozen_file() {
    for replacement in [None, Some(BROKEN_REPLACEMENT)] {
        let prompt = build(&input_of(&fixture_with(replacement)));
        assert!(prompt.instructions.contains(
            "Never ask the agent to edit a frozen file; ask only for evidence it can produce \
             read-only, or for a revised `--replacement`."
        ));
    }
}

/// The frozen test passes and the replacement does not compile: the command
/// the prompt shows runs the frozen file, so the prompt must say the
/// replacement was not run and must not present that run as its evidence.
#[test]
fn a_replacement_is_judged_by_reading_not_by_the_command_that_runs_the_frozen_file() {
    let fx = fixture_with(Some(BROKEN_REPLACEMENT));

    let prompt = build(&input_of(&fx));

    let instructions = &prompt.instructions;
    assert!(instructions.contains(
        "The commands above run the frozen content; a replacement has not been run. Judge each \
         replacement by reading it against the scenario and rejects clause of the contracts in \
         its file. Completion runs it after an accept."
    ));
    assert!(instructions.contains(
        "accept: the contract's replacement, or the `plan_patch` you give for it, still sets up \
         its `scenario` and still fails on its `rejects` implementation."
    ));
    assert!(instructions.contains("An accepted replacement is written into the worktree"));
    assert!(instructions
        .contains("contract, frozen file, proposed replacement, or diff. A citation has file"));
    for claim in [
        "the replacement passes",
        "passes with the replacement",
        "replacement run",
    ] {
        assert!(!instructions.contains(claim), "{claim}");
    }
}
