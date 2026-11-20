//! `with_admitted_overlay`: a refused admission writes nothing, and the
//! admission's guard lives through the writes only.

use std::cell::Cell;
use std::fs;
use std::path::PathBuf;

use anyhow::anyhow;
use tempfile::TempDir;

use super::{with_admitted_overlay, Relocked};

const FILE: &str = "x_contract.rs";
const FROZEN: &[u8] = b"frozen\n";

fn worktree() -> TempDir {
    let tmp = TempDir::new().expect("tempdir");
    fs::write(tmp.path().join(FILE), FROZEN).expect("write frozen");
    tmp
}

const REPLACEMENT: &[u8] = b"replacement\n";

fn overlay() -> Vec<(String, Vec<u8>)> {
    vec![(FILE.to_string(), REPLACEMENT.to_vec())]
}

/// A restore lock with no freeze record behind it.
fn no_freeze() -> anyhow::Result<Relocked<()>> {
    Ok(((), Vec::new()))
}

#[test]
fn a_refused_admission_writes_nothing_and_runs_nothing() {
    let tmp = worktree();
    let ran = Cell::new(false);
    let result: anyhow::Result<()> = with_admitted_overlay(
        tmp.path(),
        &overlay(),
        (|| Err::<(), _>(anyhow!("not the running job")), no_freeze),
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
        (|| Ok(Guard(&released)), no_freeze),
        || Ok((released.get(), fs::read(tmp.path().join(FILE))?)),
    )
    .expect("the overlay runs");
    assert_eq!(seen, (true, REPLACEMENT.to_vec()));
    assert_eq!(fs::read(tmp.path().join(FILE)).expect("read"), FROZEN);
}

/// A ruling accepted during the run rewrote the file and re-froze it at the
/// replacement bytes: the restore leaves the re-frozen bytes in place.
#[test]
fn the_restore_keeps_a_file_re_frozen_at_the_replacement() {
    let tmp = worktree();
    let refrozen = || {
        let sha = crate::utils::sha256_hex(REPLACEMENT);
        Ok(((), vec![(PathBuf::from(FILE), sha)]))
    };
    with_admitted_overlay(tmp.path(), &overlay(), (|| Ok(()), refrozen), || Ok(()))
        .expect("the overlay runs");
    assert_eq!(fs::read(tmp.path().join(FILE)).expect("read"), REPLACEMENT);
}

/// A file rewritten to other bytes during the run is not overwritten with
/// the pre-overlay snapshot, and the restore runs under its lock.
#[test]
fn the_restore_leaves_a_file_rewritten_during_the_run_under_the_lock() {
    let tmp = worktree();
    let locked = Cell::new(false);
    let relock = || {
        locked.set(true);
        no_freeze()
    };
    with_admitted_overlay(tmp.path(), &overlay(), (|| Ok(()), relock), || {
        fs::write(tmp.path().join(FILE), b"ruling\n")?;
        Ok(())
    })
    .expect("the overlay runs");
    assert!(locked.get(), "the restore must take the lock");
    assert_eq!(fs::read(tmp.path().join(FILE)).expect("read"), b"ruling\n");
}
