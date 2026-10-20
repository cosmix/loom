//! A spawn refused for a hooks directory that does not resolve names the cause.

use serial_test::serial;

use super::missing_hooks_reason;
use crate::assets::install::test_env::InstallRootsEnv;

#[test]
#[serial]
fn a_relative_home_refusal_names_home() {
    let home = tempfile::tempdir().unwrap();
    let mut env = InstallRootsEnv::isolate(home.path());
    env.set("HOME", "relative/home");

    let reason = missing_hooks_reason();
    assert!(
        reason.contains("$HOME") && reason.contains("must be absolute"),
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
