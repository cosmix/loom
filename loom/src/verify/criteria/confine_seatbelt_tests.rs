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

#[path = "confine_seatbelt_probe_tests.rs"]
mod probe;

#[path = "confine_seatbelt_loopback_tests.rs"]
mod loopback;

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
        crate::process::sandbox_probe::confined_commands_runnable(),
        test,
        "confined commands cannot start",
    )
}

/// The profile a confined `/bin/sh` in `tree` runs under with no grants.
fn profile_for(tree: &Path) -> String {
    let mut original = Command::new("/bin/sh");
    original.current_dir(tree);
    crate::process::apply_stage_environment(&mut original);
    super::seatbelt_profile(&original, tree, super::NetworkAccess::Denied).unwrap()
}

/// Seatbelt applies the last matching rule: a deny before the global
/// `(allow file-read-metadata)` would leave the files' metadata readable.
#[test]
fn the_profile_denies_every_read_of_the_cargo_files_after_allowing_metadata() {
    let (_root, tree) = tree_below_cargo_files("[package]\n", "");
    let profile = profile_for(&tree);
    let metadata = profile.find("(allow file-read-metadata)").unwrap();
    for name in ["Cargo.toml", ".cargo/config.toml"] {
        let path = tree.parent().unwrap().join(name);
        let path = serde_json::to_string(&path.to_string_lossy()).unwrap();
        for rule in [
            format!("(deny file-read* (literal {path}))"),
            format!("(deny file-read-metadata file-test-existence (literal {path}))"),
        ] {
            let at = profile.find(&rule);
            assert!(
                at.is_some_and(|at| at > metadata),
                "{rule} missing or before the metadata allowance in {profile}"
            );
        }
    }
}

#[test]
fn a_profile_naming_a_path_with_a_control_character_is_refused() {
    let root = TempDir::new().unwrap();
    let tree = root.path().canonicalize().unwrap().join("work\u{1}tree");
    std::fs::create_dir(&tree).unwrap();
    let mut original = Command::new("/bin/sh");
    original.current_dir(&tree);
    let error = super::seatbelt_profile(&original, &tree, super::NetworkAccess::Denied)
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
        "confined: {result:?}\n{}",
        probe::report(&spec, &tree)
    );
}

/// Node's package-scope lookup reads `package.json` in every ancestor and
/// treats `EPERM` as fatal, so each file above the tree must read, not be hidden.
#[test]
fn the_profile_allows_a_literal_read_of_each_package_json_above_the_tree() {
    let (_root, tree) = tree_below_cargo_files("[package]\n", "");
    let above = tree.parent().unwrap().join("package.json");
    std::fs::write(&above, "{}\n").unwrap();
    std::fs::write(tree.join("package.json"), "{}\n").unwrap();
    let profile = profile_for(&tree);
    let quoted = |path: &Path| serde_json::to_string(&path.to_string_lossy()).unwrap();
    let allow = format!("(allow file-read* (literal {}))", quoted(&above));
    assert!(profile.contains(&allow), "{allow} missing from {profile}");
    let own = format!("(literal {})", quoted(&tree.join("package.json")));
    assert!(!profile.contains(&own), "{own} named in {profile}");
    assert!(!profile.contains(&format!("(subpath {})", quoted(tree.parent().unwrap()))));
}

#[test]
fn a_package_json_above_the_tree_is_readable_inside_the_boundary() {
    if launcher_absent("a_package_json_above_the_tree_is_readable_inside_the_boundary") {
        return;
    }
    let (_root, tree) = tree_below_cargo_files("[package]\n", "");
    std::fs::write(tree.parent().unwrap().join("package.json"), "ABOVE\n").unwrap();
    let spec = CommandSpec::shell("/bin/cat ../package.json; echo DONE");
    let timeout = Duration::from_secs(15);
    let result =
        run_spec_with_timeout(&spec, Some(&tree), timeout, CommandConfinement::Confined).unwrap();
    assert!(
        result.success && result.stdout.contains("ABOVE") && result.stdout.contains("DONE"),
        "confined: {result:?}\n{}",
        probe::report(&spec, &tree)
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
        "confined: {result:?}\n{}",
        probe::report(&spec, &tree)
    );
}

/// A confined command binds and connects a Unix socket below its own
/// `TMPDIR` (sccache's client waits for its server's startup notification on
/// one), and still cannot bind one anywhere else, its writable tree included.
#[test]
fn unix_sockets_are_allowed_below_the_command_tmpdir_only() {
    if launcher_absent("unix_sockets_are_allowed_below_the_command_tmpdir_only") {
        return;
    }
    let (_root, tree) = tree_below_cargo_files("[package]\n", "");
    let perl = "/usr/bin/perl -MIO::Socket::UNIX -e '$p = shift; \
        $s = IO::Socket::UNIX->new(Type => SOCK_STREAM(), Local => $p, Listen => 1) or die \"bind: $!\\n\"; \
        IO::Socket::UNIX->new(Type => SOCK_STREAM(), Peer => $p) or die \"connect: $!\\n\"; \
        print \"SOCKET_OK\\n\"'";
    let spec = CommandSpec::shell(format!(
        "mkdir \"$TMPDIR/helper\" && {perl} \"$TMPDIR/helper/sock\"; {perl} \"$PWD/tree.sock\""
    ));
    let timeout = Duration::from_secs(15);
    let result =
        run_spec_with_timeout(&spec, Some(&tree), timeout, CommandConfinement::Confined).unwrap();
    assert_eq!(
        result.stdout.matches("SOCKET_OK").count(),
        1,
        "the TMPDIR socket must work and the tree socket must not: {result:?}"
    );
    assert!(
        result.stderr.contains("bind: Operation not permitted"),
        "{result:?}"
    );
}

/// A Unix socket listening outside the command's own `TMPDIR` stays
/// unreachable through a symlink the command plants there: Seatbelt matches
/// the socket's resolved path, not the path `connect` names. The listener sits
/// in the tree, which the command can read, so only the socket rule refuses it.
/// A hard link into `TMPDIR`, whose path the rule would match, must not
/// connect either (the profile refuses the link itself).
#[test]
fn a_tmpdir_symlink_does_not_reach_a_host_socket() {
    if launcher_absent("a_tmpdir_symlink_does_not_reach_a_host_socket") {
        return;
    }
    let (_root, tree) = tree_below_cargo_files("[package]\n", "");
    let socket = tree.join("l.sock");
    let _listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    let perl = "/usr/bin/perl -MIO::Socket::UNIX -e '$p = shift; \
        IO::Socket::UNIX->new(Type => SOCK_STREAM(), Peer => $p) or die \"connect: $!\\n\"; \
        print \"CONNECT_OK\\n\"'";
    let spec = CommandSpec::shell(format!(
        "test -r \"$PWD/l.sock\" && ln -s \"$PWD/l.sock\" \"$TMPDIR/d\" && echo LINKED && {perl} \"$TMPDIR/d\"; ln \"$PWD/l.sock\" \"$TMPDIR/h\" && echo HARD && {perl} \"$TMPDIR/h\""
    ));
    let timeout = Duration::from_secs(15);
    let result =
        run_spec_with_timeout(&spec, Some(&tree), timeout, CommandConfinement::Confined).unwrap();
    assert!(result.stdout.contains("LINKED"), "{result:?}");
    assert!(!result.stdout.contains("CONNECT_OK"), "{result:?}");
    assert!(
        result.stderr.contains("connect: Operation not permitted"),
        "{result:?}"
    );
    // The hard link itself is refused, so `HARD` never prints and `ln`
    // reports the denial; any other `ln` failure fails here.
    assert!(!result.stdout.contains("HARD"), "{result:?}");
    let ln_refused = result
        .stderr
        .lines()
        .any(|line| line.starts_with("ln: ") && line.ends_with(": Operation not permitted"));
    assert!(ln_refused, "{result:?}");
}

/// `sandbox-exec` parses every network mode's whole profile, loopback grant and
/// listings included. Exit 65 is a parse error; a sandbox that refuses to nest another
/// (71) has parsed it.
#[test]
fn every_network_mode_renders_a_profile_that_parses() {
    let (_root, tree) = tree_below_cargo_files("[package]\n", "");
    let mut original = Command::new("/bin/sh");
    original.current_dir(&tree);
    for network in [
        super::NetworkAccess::Denied,
        super::NetworkAccess::Loopback,
        super::NetworkAccess::Provision,
    ] {
        let profile = super::seatbelt_profile(&original, &tree, network).unwrap();
        let status = Command::new("/usr/bin/sandbox-exec")
            .arg("-p")
            .arg(&profile)
            .arg("/usr/bin/true")
            .status()
            .unwrap();
        assert_ne!(status.code(), Some(65), "{profile}");
    }
}
