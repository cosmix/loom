//! Git's per-user files under `$HOME/.config/git` are readable inside the
//! macOS boundary. Seen but unreadable, the ignore file made Cargo fail every
//! build script's first fingerprint ("Could not read repository exclude").
//! The test pins the process `HOME` (hence `#[serial]`): a confined command's
//! environment comes from loom's own.

use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

use crate::models::stage::CommandConfinement;
use crate::process::sandbox_probe::skip_unless;
use crate::verify::criteria::{run_spec_in_optional_tree, CommandSpec};

/// Sets `HOME` for the test's duration and restores it on drop.
struct HomeGuard(Option<OsString>);

impl HomeGuard {
    fn pin(home: &Path) -> Self {
        let saved = std::env::var_os("HOME");
        std::env::set_var("HOME", home);
        Self(saved)
    }
}

impl Drop for HomeGuard {
    fn drop(&mut self) {
        match &self.0 {
            Some(home) => std::env::set_var("HOME", home),
            None => std::env::remove_var("HOME"),
        }
    }
}

#[test]
#[serial_test::serial]
fn git_user_ignore_and_attributes_are_readable_inside_the_boundary() {
    let test = "git_user_ignore_and_attributes_are_readable_inside_the_boundary";
    if skip_unless(
        super::super::launcher().is_some(),
        test,
        "OS sandbox launcher absent",
    ) || crate::process::sandbox_probe::skip_without_confinement()
    {
        return;
    }
    let (home_dir, tree_dir) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let home = home_dir.path().canonicalize().unwrap();
    let tree = tree_dir.path().canonicalize().unwrap();
    std::fs::create_dir_all(home.join(".config/git")).unwrap();
    std::fs::write(home.join(".config/git/ignore"), "IGNORE\n").unwrap();
    std::fs::write(home.join(".config/git/attributes"), "ATTRIBUTES\n").unwrap();
    let _home = HomeGuard::pin(&home);
    let script = "cat \"$HOME/.config/git/ignore\" \"$HOME/.config/git/attributes\"; echo DONE";
    let result = run_spec_in_optional_tree(
        &CommandSpec::shell(script),
        Some(&tree),
        Some(&tree),
        Duration::from_secs(30),
        CommandConfinement::Confined,
    )
    .unwrap();
    assert!(result.stdout.contains("DONE"), "{result:?}");
    assert!(result.stdout.contains("IGNORE"), "{result:?}");
    assert!(result.stdout.contains("ATTRIBUTES"), "{result:?}");
}
