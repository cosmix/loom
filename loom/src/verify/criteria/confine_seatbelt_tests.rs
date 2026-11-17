//! Cargo files above a confined tree are absent inside the Seatbelt boundary,
//! as under bubblewrap, so Cargo's workspace and config search passes them.
//! Each assertion prints the whole command result: these run only on macOS.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use tempfile::TempDir;

use crate::models::stage::CommandConfinement;
use crate::process::sandbox_probe::skip_unless;
use crate::verify::criteria::{run_spec_with_timeout, CommandSpec};

/// A canonical `<tmp>/worktree` whose parent holds `Cargo.toml` and
/// `.cargo/config.toml` with the given contents.
fn tree_below_cargo_files(manifest: &str, config: &str) -> (TempDir, PathBuf) {
    let root = TempDir::new().unwrap();
    let parent = root.path().canonicalize().unwrap();
    std::fs::create_dir_all(parent.join(".cargo")).unwrap();
    std::fs::write(parent.join("Cargo.toml"), manifest).unwrap();
    std::fs::write(parent.join(".cargo/config.toml"), config).unwrap();
    let tree = parent.join("worktree");
    std::fs::create_dir(&tree).unwrap();
    (root, tree)
}

fn launcher_absent(test: &str) -> bool {
    skip_unless(
        super::super::launcher().is_some(),
        test,
        "OS sandbox launcher absent",
    )
}

/// The `-p` argument of the wrapped command.
fn profile_for(tree: &Path) -> String {
    let mut original = Command::new("/bin/sh");
    original.current_dir(tree);
    crate::process::apply_stage_environment(&mut original);
    let scratch = super::CargoScratch::prepare(&original, tree, false).unwrap();
    let wrapped = super::platform_command(
        &original,
        tree,
        tree,
        Path::new("/bin/sh"),
        &scratch,
        super::NetworkAccess::Denied,
        None,
    )
    .unwrap();
    let args: Vec<String> = wrapped
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    let at = args.iter().position(|arg| arg == "-p").unwrap();
    args[at + 1].clone()
}

/// Seatbelt applies the last matching rule: a deny before the global
/// `(allow file-read-metadata)` would leave the files' metadata readable.
#[test]
fn the_profile_denies_every_read_of_the_cargo_files_after_allowing_metadata() {
    if launcher_absent("the_profile_denies_every_read_of_the_cargo_files_after_allowing_metadata") {
        return;
    }
    let (_root, tree) = tree_below_cargo_files("[package]\n", "");
    let profile = profile_for(&tree);
    let metadata = profile.find("(allow file-read-metadata)").unwrap();
    for name in ["Cargo.toml", ".cargo/config.toml"] {
        let path = tree.parent().unwrap().join(name);
        let rule = format!(
            "(deny file-read* (literal {}))",
            serde_json::to_string(&path.to_string_lossy()).unwrap()
        );
        let at = profile.find(&rule);
        assert!(
            at.is_some_and(|at| at > metadata),
            "{rule} missing or before the metadata allowance in {profile}"
        );
    }
}

#[test]
fn a_profile_naming_a_path_with_a_control_character_is_refused() {
    if launcher_absent("a_profile_naming_a_path_with_a_control_character_is_refused") {
        return;
    }
    let root = TempDir::new().unwrap();
    let tree = root.path().canonicalize().unwrap().join("work\u{1}tree");
    std::fs::create_dir(&tree).unwrap();
    let mut original = Command::new("/bin/sh");
    original.current_dir(&tree);
    let scratch = super::CargoScratch::prepare(&original, &tree, false).unwrap();
    let network = super::NetworkAccess::Denied;
    let shell = Path::new("/bin/sh");
    let error = super::platform_command(&original, &tree, &tree, shell, &scratch, network, None)
        .expect_err("a control character in the tree path must be refused");
    let message = format!("{error:#}");
    assert!(message.contains("control character"), "{message}");
}

#[test]
fn cargo_files_above_the_tree_are_absent_inside_the_boundary() {
    if launcher_absent("cargo_files_above_the_tree_are_absent_inside_the_boundary") {
        return;
    }
    let (_root, tree) = tree_below_cargo_files("[package]\n", "");
    let spec = CommandSpec::shell(
        "test -e ../Cargo.toml || echo ABSENT_MANIFEST; \
         test -e ../.cargo/config.toml || echo ABSENT_CONFIG",
    );
    let timeout = Duration::from_secs(15);
    // The fixture is right: unconfined, both files exist.
    let control =
        run_spec_with_timeout(&spec, Some(&tree), timeout, CommandConfinement::Inherit).unwrap();
    assert!(
        control.success && !control.stdout.contains("ABSENT"),
        "control: {control:?}"
    );
    let result =
        run_spec_with_timeout(&spec, Some(&tree), timeout, CommandConfinement::Confined).unwrap();
    assert!(
        result.success
            && result.stdout.contains("ABSENT_MANIFEST")
            && result.stdout.contains("ABSENT_CONFIG"),
        "confined: {result:?}"
    );
}

/// A `[workspace]` above that does not list the package, and an unparsable
/// config, each stop Cargo when it can see them.
#[test]
#[serial_test::serial]
fn cargo_inside_the_boundary_ignores_the_workspace_and_config_above_it() {
    let _toolchain = crate::verify::test_support::ToolchainReadRoot::from_runner();
    if launcher_absent("cargo_inside_the_boundary_ignores_the_workspace_and_config_above_it") {
        return;
    }
    let (_root, tree) = tree_below_cargo_files("[workspace]\n", "not = [toml\n");
    std::fs::create_dir(tree.join("src")).unwrap();
    std::fs::write(
        tree.join("Cargo.toml"),
        "[package]\nname = \"confined-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::write(tree.join("src/main.rs"), "fn main() {}\n").unwrap();
    let spec = CommandSpec::program(
        "cargo",
        [
            "metadata",
            "--offline",
            "--no-deps",
            "--format-version",
            "1",
        ],
    );
    let timeout = Duration::from_secs(60);
    let control =
        run_spec_with_timeout(&spec, Some(&tree), timeout, CommandConfinement::Inherit).unwrap();
    assert!(!control.success, "control should fail: {control:?}");
    let result =
        run_spec_with_timeout(&spec, Some(&tree), timeout, CommandConfinement::Confined).unwrap();
    assert!(
        result.success && result.stdout.contains("confined-fixture"),
        "confined: {result:?}"
    );
}
