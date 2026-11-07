//! `TI-ratchet-<path>` raises only when a ratchet file loosens from base.

use super::tests::{ids, sha256_hex, Repo};
use super::*;

const LEDGER: &str = "ledger.txt";
const FLOOR: &str = "thresholds.txt";
const LEDGER_BASE: &str = "file src/a.rs 430\nfunction src/b.rs run 60\n";
const TESTS: (&str, &str) = ("src/calc_tests.rs", "#[test]\nfn adds() {}\n");

fn repo() -> Repo {
    Repo::new(&[
        TESTS,
        (LEDGER, LEDGER_BASE),
        (FLOOR, "coverage 80%\nfile src/a.rs 5\n"),
    ])
}

fn ratchet(repo: &Repo) -> Vec<IntegrityEvent> {
    repo.events(&[LEDGER.to_string(), FLOOR.to_string()])
}

#[test]
fn lowered_ledger_line_raises_nothing() {
    let repo = repo();
    repo.write(LEDGER, "file src/a.rs 420\nfunction src/b.rs run 60\n");

    assert_eq!(ids(&ratchet(&repo)), Vec::<&str>::new());
}

#[test]
fn raised_value_and_new_key_raise() {
    let raised = repo();
    raised.write(LEDGER, "file src/a.rs 431\nfunction src/b.rs run 60\n");
    assert_eq!(ids(&ratchet(&raised)), ["TI-ratchet-ledger.txt"]);

    let added = repo();
    added.write(LEDGER, &format!("{LEDGER_BASE}file src/c.rs 1\n"));
    assert_eq!(ids(&ratchet(&added)), ["TI-ratchet-ledger.txt"]);
}

#[test]
fn new_ratchet_file_at_head_raises() {
    let repo = repo();
    repo.write("extra.txt", "file src/a.rs 1\n");

    let events = repo.events(&["extra.txt".to_string()]);

    assert_eq!(ids(&events), ["TI-ratchet-extra.txt"]);
}

#[test]
fn deleted_ratchet_file_raises_with_no_hash() {
    let repo = repo();
    std::fs::remove_file(repo.root.join(LEDGER)).unwrap();

    let events = ratchet(&repo);

    assert_eq!(ids(&events), ["TI-ratchet-ledger.txt"]);
    assert_eq!(events[0].current_sha256, None);
}

#[test]
fn a_floor_line_makes_the_whole_file_unparsable_so_any_change_raises() {
    let repo = repo();
    repo.write(FLOOR, "coverage 80%\nfile src/a.rs 4\n");

    assert_eq!(ids(&ratchet(&repo)), ["TI-ratchet-thresholds.txt"]);
}

#[test]
fn ledger_event_is_pinned_to_the_file_hash() {
    let repo = repo();
    let loosened = "file src/a.rs 440\nfunction src/b.rs run 60\n";
    repo.write(LEDGER, loosened);

    let events = ratchet(&repo);

    assert_eq!(events[0].kind, EventKind::Ratchet);
    assert_eq!(events[0].current_sha256, Some(sha256_hex(loosened)));
}
