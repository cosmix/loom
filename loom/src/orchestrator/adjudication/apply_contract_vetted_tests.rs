//! An accept freezes only bytes loom vetted (`apply_contract_rulings.rs`):
//! a frozen file the verdict does not write keeps its frozen copy, in the
//! freeze and in the worktree, whether the agent edited it after admission or
//! a trial's overlay still lies over it; the apply waits for the stage's
//! host-job lock a trial lays its overlay under.

use std::sync::mpsc;
use std::time::Duration;

use super::tests::{
    notice, replacing, rule, rule_patching, three_contracts, worktree_bytes, ALPHA, BETA, GAMMA,
    HARNESS_CONTENT, HARNESS_FILE, Y_CONTENT, Y_FILE,
};
use crate::host_verify::store as job_store;
use crate::models::dispute::ContractRulingKind::{Accept, Reject};
use crate::orchestrator::adjudication::apply_contract_tests::{
    frozen_hash, replacing_contract, Fixture,
};
use crate::utils::sha256_hex;
use crate::verify::contracts::test_support::{CONTRACT_CONTENT, CONTRACT_FILE};

const GUTTED: &[u8] = b"pub fn ready() -> bool { true }\n";
const WEAK: &[u8] = b"#[test]\nfn rejects_x() {}\n";

/// The freeze and the worktree both hold `content` for `path`.
fn assert_frozen_at(fx: &Fixture, path: &str, content: &[u8]) {
    assert_eq!(frozen_hash(fx, path), Some(sha256_hex(content)), "{path}");
    assert_eq!(worktree_bytes(fx, path), content, "{path}");
}

/// The accept of `gamma`'s replacement, filed over [`Y_FILE`].
fn accept_gamma(fx: &Fixture) -> anyhow::Result<()> {
    let rulings = [(ALPHA, Reject), (BETA, Reject), (GAMMA, Accept)];
    let filed = vec![replacing(Y_FILE, Y_CONTENT)];
    rule(fx, &[ALPHA, BETA, GAMMA], filed, &rulings)
}

#[test]
fn files_the_verdict_does_not_write_keep_their_frozen_copies() {
    let fx = three_contracts();
    // Edited after admission: the harness, and the undisputed contracts' file.
    std::fs::write(fx.worktree.join(HARNESS_FILE), GUTTED).unwrap();
    std::fs::write(fx.worktree.join(CONTRACT_FILE), WEAK).unwrap();

    accept_gamma(&fx).unwrap();

    assert_frozen_at(&fx, HARNESS_FILE, HARNESS_CONTENT);
    assert_frozen_at(&fx, CONTRACT_FILE, CONTRACT_CONTENT);
    assert_frozen_at(&fx, Y_FILE, b"// replaced tests/y_contract.rs\n");
}

#[test]
fn a_disputed_file_edited_after_admission_is_not_frozen_by_a_patch_only_accept() {
    let fx = three_contracts();
    std::fs::write(fx.worktree.join(CONTRACT_FILE), WEAK).unwrap();
    let spec = "id: alpha\nfile: tests/x_contract.rs\ntest: tests::rejects_x\n\
                runner: cargo-test\nscenario: x arrives late\nrejects: accepting x\n";

    let filed = (&[ALPHA][..], Vec::new());
    rule_patching(&fx, filed, &[(ALPHA, Accept)], replacing_contract(spec)).unwrap();

    assert_frozen_at(&fx, CONTRACT_FILE, CONTRACT_CONTENT);
    assert_frozen_at(&fx, HARNESS_FILE, HARNESS_CONTENT);
}

#[test]
fn a_live_trial_overlay_is_never_frozen_and_the_apply_waits_for_its_lock() {
    let fx = three_contracts();
    // A trial of a weaker harness, its overlay laid under the stage lock.
    let lock = job_store::lock_stage(&fx.work, "s1").unwrap();
    std::fs::write(fx.worktree.join(HARNESS_FILE), GUTTED).unwrap();

    let (done, finished) = mpsc::channel();
    let applying = std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            let result = accept_gamma(&fx);
            done.send(()).unwrap();
            result
        });
        let waited = finished.recv_timeout(Duration::from_millis(300)).is_err();
        assert!(waited, "the apply must wait for the stage's host-job lock");
        assert_eq!(worktree_bytes(&fx, HARNESS_FILE), GUTTED);
        drop(lock);
        worker.join().unwrap()
    });

    applying.unwrap();
    assert_frozen_at(&fx, HARNESS_FILE, HARNESS_CONTENT);
    assert_frozen_at(&fx, Y_FILE, b"// replaced tests/y_contract.rs\n");
}

#[test]
fn an_overlay_of_the_accepted_bytes_is_frozen_as_the_replacement() {
    let fx = three_contracts();
    let replaced = b"// replaced tests/y_contract.rs\n";
    std::fs::write(fx.worktree.join(Y_FILE), replaced).unwrap();
    std::fs::write(fx.worktree.join(HARNESS_FILE), GUTTED).unwrap();

    accept_gamma(&fx).unwrap();

    assert_frozen_at(&fx, Y_FILE, replaced);
    assert_frozen_at(&fx, HARNESS_FILE, HARNESS_CONTENT);
    let record = crate::verify::contracts::store::load_freeze(&fx.work, "s1")
        .unwrap()
        .unwrap();
    assert_eq!(record.files.len(), 3, "{record:?}");
}

#[test]
fn a_reject_without_a_replacement_names_restores_only_for_a_changed_frozen_file() {
    let clean = three_contracts();
    rule(&clean, &[ALPHA], Vec::new(), &[(ALPHA, Reject)]).unwrap();
    let text = notice(&clean);
    assert!(!text.contains("contracts restore"), "{text}");
    assert!(
        text.contains("implement against the frozen contracts"),
        "{text}"
    );

    let drifted = three_contracts();
    std::fs::write(drifted.worktree.join(HARNESS_FILE), GUTTED).unwrap();
    rule(&drifted, &[ALPHA], Vec::new(), &[(ALPHA, Reject)]).unwrap();
    let text = notice(&drifted);
    assert!(
        text.contains("`loom stage contracts restore s1 --contract alpha`"),
        "{text}"
    );
}
