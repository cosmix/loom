//! Per-contract rulings applied (`apply_contract_rulings.rs`): which of the
//! filed replacements are written when rulings are mixed, a harness
//! replacement held back by one reject, and a replay after a crash between
//! two writes.

use serde_json::json;

use crate::models::dispute::{ContractRulingKind, FileReplacement};
use crate::models::stage::StageStatus;
use crate::orchestrator::adjudication::apply_contract::FAIL_AFTER_WRITE;
use crate::orchestrator::adjudication::apply_contract_tests::{
    fixture_of, frozen_hash, replacing_contract, Fixture,
};
use crate::orchestrator::adjudication::contract_verdict_fixtures::{
    contract_kind, rulings_verdict,
};
use crate::orchestrator::adjudication::tests::{write_request, write_verdict};
use crate::orchestrator::adjudication::{feedback, AdjudicatorRegistry};
use crate::plan::schema::ContractSpec;
use crate::utils::sha256_hex;
use crate::verify::contracts::test_support::{contract, CONTRACT_CONTENT, CONTRACT_FILE};
use crate::verify::transitions::{load_stage, update_stage};

use ContractRulingKind::{Accept, Reject};

pub(super) const ALPHA: &str = "alpha";
pub(super) const BETA: &str = "beta";
pub(super) const GAMMA: &str = "gamma";
pub(super) const Y_FILE: &str = "tests/y_contract.rs";
pub(super) const Y_CONTENT: &[u8] = b"#[test]\nfn rejects_y() { assert!(false); }\n";
pub(super) const HARNESS_FILE: &str = "tests/support/helper.rs";
pub(super) const HARNESS_CONTENT: &[u8] = b"pub fn ready() -> bool { false }\n";

fn spec(id: &str, file: &str) -> ContractSpec {
    ContractSpec {
        id: id.to_string(),
        file: file.to_string(),
        ..contract()
    }
}

/// `alpha` and `beta` in [`CONTRACT_FILE`], `gamma` in [`Y_FILE`], all frozen
/// with [`HARNESS_FILE`].
pub(super) fn three_contracts() -> Fixture {
    let specs = vec![
        spec(ALPHA, CONTRACT_FILE),
        spec(BETA, CONTRACT_FILE),
        spec(GAMMA, Y_FILE),
    ];
    fixture_of(
        Some("."),
        specs,
        &[(Y_FILE, Y_CONTENT), (HARNESS_FILE, HARNESS_CONTENT)],
    )
}

pub(super) fn replacing(file: &str, base: &[u8]) -> FileReplacement {
    FileReplacement {
        file: file.to_string(),
        content: format!("// replaced {file}\n"),
        base_sha256: Some(sha256_hex(base)),
    }
}

/// File dispute 1 over `ids` with `replacements`, record `rulings` and apply.
/// The verdict is written as recorded, so apply sees rulings that recording
/// would refuse (an accept a reject holds back) too.
pub(super) fn rule(
    fx: &Fixture,
    ids: &[&str],
    replacements: Vec<FileReplacement>,
    rulings: &[(&str, ContractRulingKind)],
) -> anyhow::Result<()> {
    rule_patching(fx, (ids, replacements), rulings, json!({}))
}

/// [`rule`] with the verdict's `plan_patch`.
pub(super) fn rule_patching(
    fx: &Fixture,
    (ids, replacements): (&[&str], Vec<FileReplacement>),
    rulings: &[(&str, ContractRulingKind)],
    plan_patch: serde_json::Value,
) -> anyhow::Result<()> {
    write_request(&fx.work, "s1", 1, contract_kind(ids, replacements));
    write_verdict(&fx.work, "s1", 1, rulings_verdict(rulings, plan_patch), 1);
    AdjudicatorRegistry::new().apply_verdict(&fx.work, "s1", 1)
}

pub(super) fn worktree_bytes(fx: &Fixture, rel: &str) -> Vec<u8> {
    std::fs::read(fx.worktree.join(rel)).unwrap()
}

pub(super) fn notice(fx: &Fixture) -> String {
    feedback::read_feedback(&fx.work, "s1").unwrap().unwrap()
}

#[test]
fn mixed_rulings_write_only_the_file_whose_contracts_are_all_accepted() {
    let fx = three_contracts();
    let replacements = vec![
        replacing(CONTRACT_FILE, CONTRACT_CONTENT),
        replacing(Y_FILE, Y_CONTENT),
    ];
    let rulings = [(ALPHA, Accept), (BETA, Reject), (GAMMA, Accept)];

    rule(&fx, &[ALPHA, BETA, GAMMA], replacements, &rulings).unwrap();

    assert_eq!(worktree_bytes(&fx, CONTRACT_FILE), CONTRACT_CONTENT);
    assert_eq!(
        worktree_bytes(&fx, Y_FILE),
        b"// replaced tests/y_contract.rs\n"
    );
    let replaced = sha256_hex(b"// replaced tests/y_contract.rs\n");
    assert_eq!(frozen_hash(&fx, Y_FILE), Some(replaced));
    assert_eq!(
        frozen_hash(&fx, CONTRACT_FILE),
        Some(sha256_hex(CONTRACT_CONTENT))
    );
    let text = notice(&fx);
    assert!(text.contains("- `beta` rejected:"), "{text}");
    assert!(text.contains("- `gamma` accepted:"), "{text}");
    assert!(text.contains(&format!("`git add {Y_FILE}`")), "{text}");
    assert!(
        text.contains(&format!(
            "`{CONTRACT_FILE}` (it holds contract beta, not accepted)"
        )),
        "{text}"
    );
    assert_eq!(
        load_stage("s1", &fx.work).unwrap().status,
        StageStatus::Queued
    );
}

#[test]
fn one_reject_holds_back_a_harness_replacement() {
    let fx = three_contracts();
    let replacements = vec![
        replacing(Y_FILE, Y_CONTENT),
        replacing(HARNESS_FILE, HARNESS_CONTENT),
    ];
    let rulings = [(ALPHA, Reject), (BETA, Reject), (GAMMA, Accept)];

    rule(&fx, &[ALPHA, BETA, GAMMA], replacements, &rulings).unwrap();

    assert_eq!(worktree_bytes(&fx, HARNESS_FILE), HARNESS_CONTENT);
    assert_eq!(
        frozen_hash(&fx, HARNESS_FILE),
        Some(sha256_hex(HARNESS_CONTENT))
    );
    assert_eq!(
        worktree_bytes(&fx, Y_FILE),
        b"// replaced tests/y_contract.rs\n"
    );
    let text = notice(&fx);
    assert!(
        text.contains("a harness file is written only when every ruling is an accept"),
        "{text}"
    );
}

#[test]
fn every_reject_writes_nothing_and_names_each_contract() {
    let fx = three_contracts();
    let replacements = vec![replacing(Y_FILE, Y_CONTENT)];
    let rulings = [(ALPHA, Reject), (BETA, Reject), (GAMMA, Reject)];

    rule(&fx, &[ALPHA, BETA, GAMMA], replacements, &rulings).unwrap();

    assert_eq!(worktree_bytes(&fx, Y_FILE), Y_CONTENT);
    let text = notice(&fx);
    for id in [ALPHA, BETA, GAMMA] {
        assert!(text.contains(&format!("- `{id}` rejected:")), "{text}");
    }
    assert!(!text.contains("re-frozen"), "{text}");
}

#[test]
fn a_crash_after_the_first_of_two_writes_is_repaired_by_a_replay() {
    let fx = three_contracts();
    let replacements = vec![
        replacing(CONTRACT_FILE, CONTRACT_CONTENT),
        replacing(Y_FILE, Y_CONTENT),
    ];
    let rulings = [(ALPHA, Accept), (BETA, Accept), (GAMMA, Accept)];
    FAIL_AFTER_WRITE.with(|fail| fail.set(true));

    assert!(rule(&fx, &[ALPHA, BETA, GAMMA], replacements, &rulings).is_err());
    let first = b"// replaced tests/x_contract.rs\n";
    assert_eq!(worktree_bytes(&fx, CONTRACT_FILE), first);
    assert_eq!(
        worktree_bytes(&fx, Y_FILE),
        Y_CONTENT,
        "the second write never ran"
    );
    assert_eq!(
        frozen_hash(&fx, CONTRACT_FILE),
        Some(sha256_hex(CONTRACT_CONTENT)),
        "the freeze still names the old file"
    );

    AdjudicatorRegistry::new()
        .apply_verdict(&fx.work, "s1", 1)
        .unwrap();

    assert_eq!(worktree_bytes(&fx, CONTRACT_FILE), first);
    let second = b"// replaced tests/y_contract.rs\n";
    assert_eq!(worktree_bytes(&fx, Y_FILE), second);
    assert_eq!(frozen_hash(&fx, CONTRACT_FILE), Some(sha256_hex(first)));
    assert_eq!(frozen_hash(&fx, Y_FILE), Some(sha256_hex(second)));
}

#[test]
fn a_contract_an_earlier_amendment_deleted_does_not_hold_back_its_old_file() {
    let fx = three_contracts();
    update_stage("s1", &fx.work, |stage| {
        stage.contracts.retain(|spec| spec.id != BETA);
        Ok(())
    })
    .unwrap();
    let replacements = vec![replacing(CONTRACT_FILE, CONTRACT_CONTENT)];

    rule(&fx, &[ALPHA], replacements, &[(ALPHA, Accept)]).unwrap();

    let written = b"// replaced tests/x_contract.rs\n";
    assert_eq!(worktree_bytes(&fx, CONTRACT_FILE), written);
    assert_eq!(frozen_hash(&fx, CONTRACT_FILE), Some(sha256_hex(written)));
    let text = notice(&fx);
    assert!(!text.contains("beta"), "{text}");
}

#[test]
fn a_replay_after_a_renaming_amendment_still_writes_the_accepted_replacement() {
    let specs = vec![spec(ALPHA, CONTRACT_FILE), spec(GAMMA, Y_FILE)];
    let fx = fixture_of(Some("."), specs, &[(Y_FILE, Y_CONTENT)]);
    let replacements = vec![
        replacing(Y_FILE, Y_CONTENT),
        replacing(CONTRACT_FILE, CONTRACT_CONTENT),
    ];
    let renamed = "id: alpha-v2\nfile: tests/x_contract.rs\ntest: tests::rejects_x\n\
                   runner: cargo-test\nscenario: x arrives\nrejects: accepting x\n";
    let patch = replacing_contract(renamed);
    let rulings = [(ALPHA, Accept), (GAMMA, Accept)];
    FAIL_AFTER_WRITE.with(|fail| fail.set(true));

    let filed = (&[ALPHA, GAMMA][..], replacements);
    assert!(rule_patching(&fx, filed, &rulings, patch).is_err());
    let stage = load_stage("s1", &fx.work).unwrap();
    assert_eq!(stage.contracts[0].id, "alpha-v2", "the amendment applied");
    assert_eq!(worktree_bytes(&fx, CONTRACT_FILE), CONTRACT_CONTENT);

    AdjudicatorRegistry::new()
        .apply_verdict(&fx.work, "s1", 1)
        .unwrap();

    let written = b"// replaced tests/x_contract.rs\n";
    assert_eq!(worktree_bytes(&fx, CONTRACT_FILE), written);
    assert_eq!(frozen_hash(&fx, CONTRACT_FILE), Some(sha256_hex(written)));
}
