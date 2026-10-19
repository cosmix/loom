//! A spawn refused for an install root that does not resolve names the cause.

use serial_test::serial;

use super::missing_hooks_reason;
use crate::assets::install::test_env::{InstallRootsEnv, CLAUDE_OVERRIDE};

#[test]
#[serial]
fn a_relative_claude_root_refusal_names_the_variable() {
    let home = tempfile::tempdir().unwrap();
    let mut env = InstallRootsEnv::isolate(home.path());
    env.set(CLAUDE_OVERRIDE, "relative/claude");

    let reason = missing_hooks_reason();
    assert!(
        reason.contains(CLAUDE_OVERRIDE) && reason.contains("must be an absolute path"),
        "{reason}"
    );
}

#[test]
#[serial]
fn an_absent_hooks_dir_refusal_names_where_it_was_looked_for() {
    let home = tempfile::tempdir().unwrap();
    let _env = InstallRootsEnv::isolate(home.path());

    let reason = missing_hooks_reason();
    let expected = home
        .path()
        .canonicalize()
        .unwrap()
        .join(".claude/hooks/loom");
    assert!(
        reason.starts_with("no loom hooks directory is installed")
            && reason.contains(&expected.display().to_string()),
        "{reason}"
    );
}
