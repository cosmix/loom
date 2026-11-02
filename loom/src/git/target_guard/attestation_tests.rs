//! `attestation_mode` and the record's attestation latch.

use super::test_support::{activate, git, repo};
use super::tests::{attest, commit, guard, held, record};
use super::*;

fn off_reason(root: &Path, work: &Path) -> String {
    match attestation_mode(root, work) {
        AttestationMode::Off { reason } => reason,
        AttestationMode::Active => panic!("expected attestation off"),
    }
}

fn is_unattested(state: GuardState) -> bool {
    matches!(&held(state).reasons[..], [HoldReason::Unattested { .. }])
}

#[test]
fn the_installed_hook_with_this_state_directory_is_active() {
    let repo = repo();
    activate(&repo.root);

    assert_eq!(
        attestation_mode(&repo.root, &repo.work),
        AttestationMode::Active
    );
}

/// The hook in `.git/hooks` proves nothing once git runs hooks from elsewhere.
#[test]
fn a_hooks_path_without_the_hook_turns_attestation_off() {
    let repo = repo();
    activate(&repo.root);
    git(&repo.root, &["config", "core.hooksPath", "loom/.githooks"]);

    let reason = off_reason(&repo.root, &repo.work);
    assert!(
        reason.contains("core.hooksPath is set to loom/.githooks"),
        "{reason}"
    );
    assert!(reason.contains("not installed there"), "{reason}");
}

#[test]
fn the_real_hook_in_a_repository_hooks_path_attests_a_host_commit() {
    let repo = repo();
    git(&repo.root, &["config", "core.hooksPath", "local-hooks"]);
    crate::git::hooks::install_reference_transaction_hook(&repo.root).unwrap();
    assert!(repo
        .root
        .join("local-hooks/reference-transaction")
        .is_file());
    assert_eq!(
        attestation_mode(&repo.root, &repo.work),
        AttestationMode::Active
    );
    record(&repo);

    let moved = commit(&repo.root, "src/x.rs");

    assert_eq!(guard(&repo), GuardState::Clear { accepted: moved });
}

/// The same move without the hook has no ledger line, so the mode above is what
/// cleared it.
#[test]
fn a_host_commit_that_skips_the_hook_in_a_repository_hooks_path_holds() {
    let repo = repo();
    git(&repo.root, &["config", "core.hooksPath", "local-hooks"]);
    crate::git::hooks::install_reference_transaction_hook(&repo.root).unwrap();
    record(&repo);

    std::fs::write(repo.root.join("x.rs"), "x").unwrap();
    git(&repo.root, &["add", "x.rs"]);
    git(
        &repo.root,
        &["-c", "core.hooksPath=/dev/null", "commit", "-q", "-m", "x"],
    );

    assert!(is_unattested(guard(&repo)));
}

/// A marked hook does not turn attestation on from another repository's
/// directory, from tracked content, or through a link.
#[test]
fn a_hook_loom_may_not_install_at_turns_attestation_off() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let install = |dir: &Path| {
        std::fs::create_dir_all(dir).unwrap();
        let hook = dir.join("reference-transaction");
        std::fs::write(&hook, format!("#!/bin/sh\n# {HOOK_MARKER}\nexit 0\n")).unwrap();
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    };
    let outside = tempfile::TempDir::new().unwrap();
    install(outside.path());
    let repo = repo();
    git(
        &repo.root,
        &["config", "core.hooksPath", outside.path().to_str().unwrap()],
    );
    assert!(off_reason(&repo.root, &repo.work).contains("loom cannot rely on a hook"));

    install(&repo.root.join("tracked-hooks"));
    git(&repo.root, &["add", "tracked-hooks"]);
    git(&repo.root, &["config", "core.hooksPath", "tracked-hooks"]);
    assert!(off_reason(&repo.root, &repo.work).contains("loom cannot rely on a hook"));

    symlink(outside.path(), repo.root.join("linked-hooks")).unwrap();
    git(&repo.root, &["config", "core.hooksPath", "linked-hooks"]);
    assert!(off_reason(&repo.root, &repo.work).contains("loom cannot rely on a hook"));
}

#[test]
fn a_missing_or_foreign_hook_turns_attestation_off() {
    let repo = repo();
    let reason = off_reason(&repo.root, &repo.work);
    assert!(reason.contains("hook is not installed"), "{reason}");

    let hook = repo.root.join(".git/hooks/reference-transaction");
    std::fs::create_dir_all(hook.parent().unwrap()).unwrap();
    std::fs::write(hook, "#!/bin/sh\nexit 0\n").unwrap();
    let reason = off_reason(&repo.root, &repo.work);
    assert!(reason.contains("hook is not installed"), "{reason}");
}

#[test]
fn a_hook_git_cannot_execute_turns_attestation_off() {
    use std::os::unix::fs::PermissionsExt;
    let repo = repo();
    activate(&repo.root);
    let hook = repo.root.join(".git/hooks/reference-transaction");
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o644)).unwrap();

    let reason = off_reason(&repo.root, &repo.work);
    assert!(reason.contains("not executable"), "{reason}");
}

#[test]
fn another_state_directory_turns_attestation_off() {
    let repo = repo();
    activate(&repo.root);
    let elsewhere = tempfile::TempDir::new().unwrap();

    let reason = off_reason(&repo.root, elsewhere.path());
    assert!(
        reason.contains("cannot find this state directory"),
        "{reason}"
    );
}

#[test]
fn a_latched_entry_still_holds_an_unattested_move_after_a_downgrade() {
    let repo = repo();
    activate(&repo.root);
    record(&repo);
    assert!(attestation_latched(&repo.work, "main").unwrap());
    git(&repo.root, &["config", "core.hooksPath", "/dev/null"]);
    off_reason(&repo.root, &repo.work);

    commit(&repo.root, "src/x.rs");

    assert!(is_unattested(guard(&repo)));
    assert!(attestation_latched(&repo.work, "main").unwrap());
}

#[test]
fn accept_while_off_lowers_the_latch() {
    let repo = repo();
    activate(&repo.root);
    let a = record(&repo);
    git(&repo.root, &["config", "core.hooksPath", "/dev/null"]);
    let x = commit(&repo.root, "src/x.rs");
    assert!(is_unattested(guard(&repo)));

    let accepted = accept(&repo.root, &repo.work, "main", &x).unwrap();

    assert_eq!(accepted, Accepted { from: a, to: x });
    assert!(!attestation_latched(&repo.work, "main").unwrap());
    let y = commit(&repo.root, "src/y.rs");
    assert_eq!(guard(&repo), GuardState::Clear { accepted: y });
}

#[test]
fn accept_while_active_keeps_the_latch() {
    let repo = repo();
    activate(&repo.root);
    record(&repo);
    let x = commit(&repo.root, "src/x.rs");
    assert!(is_unattested(guard(&repo)));

    accept(&repo.root, &repo.work, "main", &x).unwrap();

    assert!(attestation_latched(&repo.work, "main").unwrap());
    git(&repo.root, &["config", "core.hooksPath", "/dev/null"]);
    commit(&repo.root, "src/y.rs");
    assert!(is_unattested(guard(&repo)));
}

#[test]
fn an_unlatched_entry_latches_once_the_mode_turns_active() {
    let repo = repo();
    let a = record(&repo);
    assert!(!attestation_latched(&repo.work, "main").unwrap());
    activate(&repo.root);
    let b = commit(&repo.root, "src/b.rs");
    attest(&repo.work, &a, &b);

    assert_eq!(guard(&repo), GuardState::Clear { accepted: b });
    assert!(attestation_latched(&repo.work, "main").unwrap());
}

#[test]
fn pending_hold_walks_the_ledger_while_latched() {
    let repo = repo();
    activate(&repo.root);
    record(&repo);
    git(&repo.root, &["config", "core.hooksPath", "/dev/null"]);
    commit(&repo.root, "src/x.rs");

    let hold = pending_hold(&repo.root, &repo.work, "main")
        .unwrap()
        .unwrap();

    assert!(matches!(&hold.reasons[..], [HoldReason::Unattested { .. }]));
    assert_eq!(recorded_hold(&repo.work, "main").unwrap(), None);
}
