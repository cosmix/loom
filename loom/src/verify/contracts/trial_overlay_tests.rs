//! `with_admitted_overlay`: a refused admission writes nothing, and the
//! admission's guard lives through the writes only.

use std::cell::Cell;
use std::fs;

use anyhow::anyhow;
use tempfile::TempDir;

use super::with_admitted_overlay;

const FILE: &str = "x_contract.rs";
const FROZEN: &[u8] = b"frozen\n";

fn worktree() -> TempDir {
    let tmp = TempDir::new().expect("tempdir");
    fs::write(tmp.path().join(FILE), FROZEN).expect("write frozen");
    tmp
}

fn overlay() -> Vec<(String, Vec<u8>)> {
    vec![(FILE.to_string(), b"replacement\n".to_vec())]
}

#[test]
fn a_refused_admission_writes_nothing_and_runs_nothing() {
    let tmp = worktree();
    let ran = Cell::new(false);
    let result: anyhow::Result<()> = with_admitted_overlay(
        tmp.path(),
        &overlay(),
        || Err::<(), _>(anyhow!("not the running job")),
        || {
            ran.set(true);
            Ok(())
        },
    );
    let error = result.expect_err("the admission refused");
    assert!(
        format!("{error:#}").contains("not the running job"),
        "{error:#}"
    );
    assert!(!ran.get());
    assert_eq!(fs::read(tmp.path().join(FILE)).expect("read"), FROZEN);
}

/// Sets its flag when dropped.
struct Guard<'a>(&'a Cell<bool>);

impl Drop for Guard<'_> {
    fn drop(&mut self) {
        self.0.set(true);
    }
}

#[test]
fn the_admission_guard_is_released_after_the_writes_and_before_the_run() {
    let tmp = worktree();
    let released = Cell::new(false);
    let seen = with_admitted_overlay(
        tmp.path(),
        &overlay(),
        || Ok(Guard(&released)),
        || Ok((released.get(), fs::read(tmp.path().join(FILE))?)),
    )
    .expect("the overlay runs");
    assert_eq!(seen, (true, b"replacement\n".to_vec()));
    assert_eq!(fs::read(tmp.path().join(FILE)).expect("read"), FROZEN);
}
