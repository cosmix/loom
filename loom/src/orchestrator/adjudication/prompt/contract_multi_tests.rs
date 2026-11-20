//! The contract briefing of one filing that disputes several contracts: every
//! contract is shown, each file once, and each replacement under its own
//! heading.

use super::tests::{contract_fixture, freeze, input_of, ContractFixture};
use super::*;
use crate::models::dispute::FileReplacement;
use crate::orchestrator::adjudication::prompt::truncate::{truncate_to_budget, TRUNCATION_MARKER};
use crate::orchestrator::adjudication::prompt::MAX_PROMPT_BYTES;

fn spec(id: &str, file: &str) -> ContractSpec {
    ContractSpec {
        id: id.to_string(),
        file: file.to_string(),
        test: Some(format!("tests::{}", id.replace('-', "_"))),
        runner: Some("cargo-test".to_string()),
        cwd: None,
        scenario: format!("{id} arrives"),
        rejects: format!("accepting {id}"),
    }
}

fn replacement(file: &str, content: &str) -> FileReplacement {
    FileReplacement {
        file: file.to_string(),
        content: content.to_string(),
        base_sha256: None,
    }
}

/// A freeze holding `x_contract.rs` (contracts `rejects-x` and `rejects-y`),
/// `z_contract.rs` (contract `rejects-z`) and a harness file, with every
/// contract disputed and `replacements` attached.
fn fixture(frozen: [&str; 2], replacements: Vec<FileReplacement>) -> ContractFixture {
    let mut fx = contract_fixture();
    fx.stage.contracts = vec![
        spec("rejects-x", "tests/x_contract.rs"),
        spec("rejects-y", "tests/x_contract.rs"),
        spec("rejects-z", "tests/z_contract.rs"),
    ];
    freeze(
        &fx.work_dir,
        &[
            ("tests/x_contract.rs", frozen[0]),
            ("tests/z_contract.rs", frozen[1]),
            ("tests/support/h.rs", "pub fn h() {}\n"),
        ],
    );
    fx.request.kind = DisputeKind::Contract {
        contract_ids: ["rejects-x", "rejects-y", "rejects-z"]
            .map(str::to_string)
            .to_vec(),
        replacements,
    };
    fx
}

#[test]
fn every_disputed_contract_is_shown_and_each_file_once() {
    let fx = fixture(["x frozen\n", "z frozen\n"], Vec::new());

    let prompt = build(&input_of(&fx));

    let evidence = &prompt.evidence;
    assert!(evidence.contains("Contract IDs: rejects-x, rejects-y, rejects-z"));
    for id in ["rejects-x", "rejects-y", "rejects-z"] {
        assert!(evidence.contains(&format!("id: {id}\n")), "{id}");
        assert!(
            prompt
                .instructions
                .contains(&format!("### Contract `{id}`")),
            "{id}"
        );
    }
    assert_eq!(evidence.matches("## Frozen content").count(), 2);
    assert!(evidence.contains("File: `tests/x_contract.rs` (contracts: rejects-x, rejects-y)"));
    assert!(evidence.contains("File: `tests/z_contract.rs` (contracts: rejects-z)"));
    assert!(evidence.contains("x frozen\n") && evidence.contains("z frozen\n"));
    assert!(!evidence.contains("## Proposed replacement"));
}

#[test]
fn each_replaced_file_has_a_section_and_a_diff_naming_its_contracts() {
    let fx = fixture(
        ["x frozen\n", "z frozen\n"],
        vec![
            replacement("tests/x_contract.rs", "x replaced\n"),
            replacement("tests/support/h.rs", "pub fn h2() {}\n"),
        ],
    );

    let prompt = build(&input_of(&fx));

    let evidence = &prompt.evidence;
    assert_eq!(evidence.matches("## Proposed replacement").count(), 2);
    assert_eq!(
        evidence.matches("## Diff (frozen vs replacement)").count(),
        2
    );
    assert!(evidence.contains(
        "## Proposed replacement\n\nFile: `tests/x_contract.rs` (contracts: rejects-x, rejects-y)"
    ));
    assert!(evidence
        .contains("## Proposed replacement\n\nFile: `tests/support/h.rs` (no disputed contract"));
    assert!(evidence.contains("+x replaced"), "{evidence}");
    assert!(evidence.contains("+pub fn h2() {}"), "{evidence}");
    assert!(prompt.instructions.contains(
        "A file holding several contracts is written only when all of them are accepted."
    ));
}

/// Several replaced files must each be cut by the byte cap, and the
/// frozen-set section at the tail must survive.
#[test]
fn the_cap_cuts_every_replacement_before_it_reaches_the_frozen_set() {
    let big = |tag: &str| -> String {
        (0..2000)
            .map(|n| format!("assert!({tag}_{n}());\n"))
            .collect()
    };
    let (x, z) = (big("x"), big("z"));
    let fx = fixture(
        [&x, &z],
        vec![
            replacement("tests/x_contract.rs", &x.replace("x_7()", "other()")),
            replacement("tests/z_contract.rs", &z.replace("z_7()", "other()")),
        ],
    );
    let mut prompt = build(&input_of(&fx));
    assert!(
        prompt.total_len() > MAX_PROMPT_BYTES,
        "the fixture must overflow"
    );

    truncate_to_budget(&mut prompt);

    assert!(prompt.total_len() <= MAX_PROMPT_BYTES);
    assert!(prompt
        .evidence
        .contains("## Other frozen files changed since the freeze"));
    for part in prompt.evidence.split("## Proposed replacement").skip(1) {
        let section = part.split("\n## ").next().unwrap();
        assert!(
            section.contains(TRUNCATION_MARKER),
            "an untouched replacement"
        );
    }
}
