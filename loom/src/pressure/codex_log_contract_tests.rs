//! Contract: the Codex log is never opened through a symlink. A symlink at the log path makes
//! `spawn_codex_logged` fail, leaves the link in place and leaves its target untouched. Rejects
//! `File::create` (truncates the link target) and removing whatever sits at the path before
//! recreating it (replaces the link instead of refusing it).

use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;

use super::codex_run::spawn_codex_logged;

#[test]
fn a_symlinked_codex_log_is_refused_and_its_target_kept() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().canonicalize().unwrap();
    let victim = dir.join("victim");
    let log = dir.join("codex.log");
    fs::write(&victim, "keep").unwrap();
    symlink(&victim, &log).unwrap();

    let spawned = spawn_codex_logged(
        Path::new("/bin/sh"),
        vec!["-c".into(), "echo out".into()],
        &dir,
        &log,
        |_| {},
    );
    let refused = spawned.is_err();
    if let Ok(mut child) = spawned {
        // Settle the child so the assertions below see its final writes.
        child.wait().unwrap();
    }

    assert!(refused, "the log was opened through a symlink");
    assert_eq!(fs::read_to_string(&victim).unwrap(), "keep");
    let kind = log.symlink_metadata().unwrap().file_type();
    assert!(kind.is_symlink(), "the symlink was replaced");
}
