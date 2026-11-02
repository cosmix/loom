//! What the daemon says about the target guard at startup: the hook install
//! outcome and, while attestation is off, what the guard still holds.

use crate::git::hooks::HookInstall;

use super::{attestation_off_line, hook_install_line};

const REASON: &str = "loom's reference-transaction hook is not installed";

#[test]
fn a_hook_loom_installed_or_found_current_is_only_logged() {
    assert_eq!(
        hook_install_line(&Ok(HookInstall::Installed)),
        Ok("target guard: reference-transaction hook installed")
    );
    assert_eq!(
        hook_install_line(&Ok(HookInstall::UpToDate)),
        Ok("target guard: reference-transaction hook up to date")
    );
}

#[test]
fn a_foreign_hook_warns_the_operator() {
    let line = hook_install_line(&Ok(HookInstall::ForeignHookPresent)).unwrap_err();

    assert!(line.contains("belongs to another tool"), "{line}");
    assert!(line.contains("left in place"), "{line}");
    // `core.hooksPath` can put the hook anywhere.
    assert!(!line.contains(".git/hooks"), "{line}");
}

#[test]
fn a_failed_hook_install_warns_with_the_error() {
    let failed = Err(anyhow::anyhow!("permission denied"));

    let line = hook_install_line(&failed).unwrap_err();

    assert!(line.contains("could not install"), "{line}");
    assert!(line.contains("permission denied"), "{line}");
}

#[test]
fn attestation_off_without_the_latch_names_what_is_still_held() {
    let line = attestation_off_line(REASON, &Ok(false));

    assert!(line.contains(REASON), "{line}");
    let held = "only control-path changes, rewrites of the target and unmerged stage work";
    assert!(line.contains(held), "{line}");
    assert!(!line.contains("recorded it on"), "{line}");
}

#[test]
fn attestation_off_with_the_latch_says_every_unattested_move_holds() {
    let line = attestation_off_line(REASON, &Ok(true));

    assert!(line.contains(REASON), "{line}");
    assert!(line.contains("this run recorded it on"), "{line}");
    assert!(line.contains("until loom target accept"), "{line}");
    assert!(!line.contains("only control-path changes"), "{line}");
}

#[test]
fn attestation_off_with_an_unreadable_record_names_the_error() {
    let unreadable = Err(anyhow::anyhow!("target-guard.json does not parse"));

    let line = attestation_off_line(REASON, &unreadable);

    assert!(line.contains(REASON), "{line}");
    let named = "the guard record could not be read: target-guard.json does not parse";
    assert!(line.contains(named), "{line}");
}
