//! The provision gate and tracked files: a provision that modifies one blocks the stage.

use super::*;

#[test]
#[serial_test::serial]
fn a_provision_that_modifies_a_tracked_file_blocks_and_names_its_full_path() {
    if !confined_commands_runnable() {
        return;
    }
    let repo = repo_tracking("web/bun.lock");

    let (passed, stage) = gate(&["echo changed >> web/bun.lock"], Vec::new(), repo.path());

    assert!(!passed);
    assert_eq!(stage.status, StageStatus::Blocked);
    let reason = stage.close_reason.expect("the block records its reason");
    // The status entry is " M web/bun.lock": a trimmed listing clips it to "eb/bun.lock".
    assert!(reason.contains("worktree: web/bun.lock;"), "{reason}");
}

#[test]
#[serial_test::serial]
fn a_pre_existing_modified_tracked_file_is_not_a_new_entry() {
    if !confined_commands_runnable() {
        return;
    }
    let repo = repo_tracking("a.txt");
    fs::write(repo.path().join("a.txt"), "edited before provisioning\n").unwrap();

    let (passed, stage) = gate(
        &["mkdir -p node_modules && touch node_modules/x"],
        Vec::new(),
        repo.path(),
    );

    assert!(passed);
    assert_eq!(stage.status, StageStatus::Queued);
}

/// A lossy decode reads the name as `bad\u{FFFD}name`, a new entry it then blocks on
/// under a different name; a skipped comparison lets the stage spawn. APFS
/// rejects such a name (EILSEQ), so the provision cannot write it on macOS.
#[cfg(not(target_os = "macos"))]
#[test]
#[serial_test::serial]
fn a_provision_that_writes_a_non_utf8_name_blocks_the_stage() {
    if !confined_commands_runnable() {
        return;
    }
    let repo = repo_ignoring_node_modules();

    let (passed, stage) = gate(
        &["touch \"$(printf 'bad\\377name')\""],
        Vec::new(),
        repo.path(),
    );

    assert!(!passed);
    let reason = stage.close_reason.expect("the block records its reason");
    assert_eq!(reason, NON_UTF8_REASON);
}
