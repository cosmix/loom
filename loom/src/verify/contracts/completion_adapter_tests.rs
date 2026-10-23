//! Which adapter makes a contract without `runner` a Rust one at completion:
//! the one its freeze recorded, never one re-detected from the worktree.

use super::tests::{fixture_with, frozen_at, run_check, Fixture, Replay, ONE_PASS};
use crate::verify::contracts::store::FrozenContract;
use crate::verify::contracts::test_support::write_files;

const FILE: &str = "src/contract_tests.rs";

/// A contract without `runner` whose module `src/lib.rs` redirects with
/// `#[path]`, frozen with `adapter` recorded for it. The worktree's
/// `Cargo.toml` owns the file, so detection would find `cargo-test`.
fn redirected(adapter: Option<&str>) -> Fixture {
    let mut fx = fixture_with(vec![FrozenContract {
        adapter: adapter.map(str::to_string),
        file: Some(FILE.to_string()),
        ..frozen_at(".")
    }]);
    write_files(
        &fx.worktree,
        &[
            (
                "src/lib.rs",
                "#[path = \"elsewhere.rs\"]\nmod contract_tests;\n",
            ),
            (FILE, "#[test]\nfn rejects_x() {}\n"),
        ],
    );
    let contract = &mut fx.stage.contracts[0];
    contract.file = FILE.to_string();
    contract.test = "contract_tests::rejects_x".to_string();
    contract.runner = None;
    fx
}

#[test]
fn a_recorded_rust_adapter_checks_the_chain() {
    let fx = redirected(Some("cargo-test"));

    let error = run_check(&fx, &Replay(ONE_PASS)).unwrap_err().to_string();

    assert!(
        error.contains("src/lib.rs declares contract_tests with #[path"),
        "{error}"
    );
}

#[test]
fn a_contract_recorded_without_an_adapter_is_not_re_detected_as_rust() {
    let fx = redirected(None);

    run_check(&fx, &Replay(ONE_PASS)).unwrap();
}
