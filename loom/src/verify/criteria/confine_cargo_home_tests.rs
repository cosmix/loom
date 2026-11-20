//! Cargo fingerprints registry packages by their absolute source path, so every
//! confined command must see the host's own Cargo home path, the one the
//! session's builds use: any other path rebuilds every dependency ("the path to
//! the source changed"), and a target directory shared with the session's
//! builds alternates between two builds.
use super::NetworkAccess;
use std::path::{Path, PathBuf};
use std::process::Command;

fn launcher_available(name: &str) -> bool {
    !crate::process::sandbox_probe::skip_unless(
        crate::process::sandbox_probe::confined_commands_runnable(),
        name,
        "confined commands cannot start",
    )
}

fn effective_cargo_home(command: &Command) -> PathBuf {
    command
        .get_envs()
        .find_map(|(key, value)| (key == "CARGO_HOME").then_some(value).flatten())
        .map(PathBuf::from)
        .expect("a confined command always names its Cargo home")
}

/// A host Cargo home with one extracted registry source.
fn host_cargo_home(root: &Path) -> PathBuf {
    let cargo = root.join("cargo");
    std::fs::create_dir_all(cargo.join("registry/src")).unwrap();
    std::fs::write(cargo.join("registry/src/sentinel"), "extracted").unwrap();
    std::fs::create_dir_all(cargo.join("git")).unwrap();
    std::fs::write(cargo.join("credentials.toml"), "secret").unwrap();
    cargo
}

fn confined_shell(tree: &Path, cargo: &Path, line: &str) -> Command {
    let mut original = Command::new("/bin/bash");
    original
        .current_dir(tree)
        .args(["--noprofile", "--norc", "-c", line]);
    crate::process::apply_stage_environment(&mut original);
    original.env("CARGO_HOME", cargo);
    original
}

#[test]
fn every_confined_command_for_one_tree_shares_one_cargo_home() {
    if !launcher_available("every_confined_command_for_one_tree_shares_one_cargo_home") {
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let tree = root.path().join("worktree");
    std::fs::create_dir(&tree).unwrap();
    let cargo = host_cargo_home(root.path());
    let original = confined_shell(&tree, &cargo, "true");
    let (first, first_scratch, _) =
        super::wrap(&original, None, NetworkAccess::Denied, None).unwrap();
    let (second, second_scratch, _) =
        super::wrap(&original, None, NetworkAccess::Denied, None).unwrap();
    assert_ne!(first_scratch.path(), second_scratch.path());
    let home = effective_cargo_home(&first);
    assert_eq!(home, effective_cargo_home(&second));
    assert_eq!(
        home, cargo,
        "the session builds against the host Cargo home"
    );
}

/// Seatbelt cannot remap the host Cargo home, so the command reads it in
/// place: sources and Cargo's state files readable, nothing writable, the
/// config and credentials files absent.
#[cfg(target_os = "macos")]
#[test]
fn macos_cargo_home_is_the_host_path_read_only() {
    if !launcher_available("macos_cargo_home_is_the_host_path_read_only") {
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let tree = root.path().join("worktree");
    std::fs::create_dir(&tree).unwrap();
    let cargo = host_cargo_home(root.path());
    std::fs::write(cargo.join("config.toml"), "[build]\njobs = 1\n").unwrap();
    std::fs::write(cargo.join(".package-cache"), "").unwrap();
    let line = r#"set -eu
[[ $(cat "$CARGO_HOME/registry/src/sentinel") == extracted ]]
cat "$CARGO_HOME/.package-cache"
printf changed 2>/dev/null > "$CARGO_HOME/registry/src/sentinel" && exit 1
touch "$CARGO_HOME/git/new" 2>/dev/null && exit 1
touch "$CARGO_HOME/.package-cache" "$CARGO_HOME/.global-cache" 2>/dev/null && exit 1
[[ -e "$CARGO_HOME/config.toml" || -e "$CARGO_HOME/credentials.toml" ]] && exit 1
printf CARGO_HOME_PASSED"#;
    let original = confined_shell(&tree, &cargo, line);
    let (mut wrapped, scratch, _) =
        super::wrap(&original, None, NetworkAccess::Denied, None).unwrap();
    assert_eq!(effective_cargo_home(&wrapped), cargo);
    scratch.populate().unwrap();
    let output = wrapped.output().unwrap();
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("CARGO_HOME_PASSED"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!cargo.join(".global-cache").exists());
    assert!(!cargo.join("git/new").exists());
}

/// The bind sequence that keeps the host path: a private writable directory
/// at the Cargo home, then the host registry read-only over it.
#[cfg(target_os = "linux")]
#[test]
fn linux_cargo_home_keeps_the_host_path_with_read_only_sources() {
    if !launcher_available("linux_cargo_home_keeps_the_host_path_with_read_only_sources") {
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let tree = root.path().join("worktree");
    std::fs::create_dir(&tree).unwrap();
    let cargo = host_cargo_home(root.path());
    let line = r#"set -eu
[[ $(cat "$CARGO_HOME/registry/src/sentinel") == extracted ]]
[[ ! -L "$CARGO_HOME/registry" ]]
printf changed 2>/dev/null > "$CARGO_HOME/registry/src/sentinel" && exit 1
touch "$CARGO_HOME/git/new" 2>/dev/null && exit 1
cat "$CARGO_HOME/credentials.toml" 2>/dev/null && exit 1
touch "$CARGO_HOME/.package-cache" "$CARGO_HOME/.package-cache-mutate" "$CARGO_HOME/.global-cache"
printf CARGO_HOME_PASSED"#;
    let original = confined_shell(&tree, &cargo, line);
    let wrap = super::wrap(&original, None, NetworkAccess::Denied, None);
    let (mut wrapped, scratch, _) = wrap.unwrap();
    assert_eq!(effective_cargo_home(&wrapped), cargo);
    let args: Vec<&std::ffi::OsStr> = wrapped.get_args().collect();
    let mount = args
        .windows(3)
        .position(|w| w[0] == "--bind" && w[2] == cargo.as_os_str())
        .expect("a private directory is bound at the host Cargo home");
    let registry = cargo.join("registry");
    let sources = args
        .windows(3)
        .position(|w| w[0] == "--ro-bind" && w[1] == registry.as_os_str() && w[2] == w[1])
        .expect("the host registry is bound read-only at its own path");
    assert!(mount < sources, "read-only sources must cover the mount");
    scratch.populate().unwrap();
    let output = wrapped.output().unwrap();
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("CARGO_HOME_PASSED"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(cargo.join("registry/src/sentinel")).unwrap(),
        "extracted"
    );
    for written in [".package-cache", ".package-cache-mutate", ".global-cache"] {
        assert!(!cargo.join(written).exists(), "{written} reached the host");
    }
    let entries: Vec<_> = std::fs::read_dir(&tree).unwrap().collect();
    assert!(entries.is_empty(), "nothing may appear inside the tree");
}

/// Removes a test's per-tree Cargo home, which outlives its scratch.
#[cfg(unix)]
struct LinkedHome(PathBuf);

#[cfg(unix)]
impl Drop for LinkedHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A command whose tree lies inside its Cargo home, so the home is linked.
#[cfg(unix)]
fn linked_scratch(cargo: &Path) -> (super::CargoScratch, LinkedHome, Command) {
    let tree = cargo.join("worktree");
    std::fs::create_dir_all(&tree).unwrap();
    let mut command = Command::new("true");
    command.current_dir(&tree).env("CARGO_HOME", cargo);
    let tree = tree.canonicalize().unwrap();
    let scratch = super::CargoScratch::prepare(&command, &tree, false, false).unwrap();
    let home = scratch.linked_cargo_home().unwrap().to_path_buf();
    (scratch, LinkedHome(home), command)
}

#[cfg(unix)]
fn private_directory(path: &Path, mode: u32) {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new().mode(mode).create(path).unwrap();
}

#[cfg(unix)]
#[test]
fn populating_a_linked_home_removes_everything_it_does_not_manage() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let cargo = host_cargo_home(root.path());
    let (scratch, home, _) = linked_scratch(&cargo);
    let outside = root.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("kept"), "kept").unwrap();
    private_directory(&home.0, 0o755);
    for file in ["config.toml", "config", "credentials.toml", "credentials"] {
        std::fs::write(home.0.join(file), "[target.x]\nrunner = \"true\"").unwrap();
    }
    std::fs::write(home.0.join(".package-cache"), "").unwrap();
    std::os::unix::fs::symlink(&outside, home.0.join(".global-cache")).unwrap();
    std::fs::create_dir_all(home.0.join("nested/closed")).unwrap();
    std::os::unix::fs::symlink(&outside, home.0.join("nested/out")).unwrap();
    std::fs::set_permissions(home.0.join("nested"), PermissionsExt::from_mode(0o500)).unwrap();
    scratch.populate().unwrap();
    let mut names: Vec<_> = std::fs::read_dir(&home.0)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert_eq!(names, [".package-cache", "git", "registry"]);
    let mode = std::fs::metadata(&home.0).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o700);
    assert_eq!(
        std::fs::read_to_string(outside.join("kept")).unwrap(),
        "kept"
    );
}

#[cfg(unix)]
#[test]
fn populating_a_linked_home_replaces_a_real_source_directory_with_its_link() {
    let root = tempfile::tempdir().unwrap();
    let cargo = host_cargo_home(root.path());
    let (scratch, home, _) = linked_scratch(&cargo);
    private_directory(&home.0, 0o700);
    std::fs::create_dir_all(home.0.join("registry/src")).unwrap();
    std::fs::write(home.0.join("registry/src/sentinel"), "tampered").unwrap();
    scratch.populate().unwrap();
    assert_eq!(
        std::fs::read_link(home.0.join("registry")).unwrap(),
        cargo.join("registry")
    );
    assert_eq!(
        std::fs::read_to_string(home.0.join("registry/src/sentinel")).unwrap(),
        "extracted"
    );
}

#[cfg(unix)]
#[test]
fn a_linked_home_that_is_a_symlink_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let cargo = host_cargo_home(root.path());
    let (scratch, home, _) = linked_scratch(&cargo);
    let elsewhere = root.path().join("elsewhere");
    private_directory(&elsewhere, 0o700);
    std::fs::write(elsewhere.join("config.toml"), "kept").unwrap();
    std::os::unix::fs::symlink(&elsewhere, &home.0).unwrap();
    let error = scratch.populate().unwrap_err();
    assert!(format!("{error:#}").contains("not a symlink"), "{error:#}");
    assert!(elsewhere.join("config.toml").exists());
    assert!(!elsewhere.join("registry").exists());
}

/// Bubblewrap binds the Cargo sources at the paths `read_roots` names; the
/// linked home must point at those paths even when the Cargo home is a symlink.
#[cfg(unix)]
#[test]
fn linked_sources_point_at_the_roots_the_launcher_grants() {
    let root = tempfile::tempdir().unwrap();
    let cargo = host_cargo_home(root.path());
    let alias = root.path().join("cargo-alias");
    std::os::unix::fs::symlink(&cargo, &alias).unwrap();
    let (scratch, home, command) = linked_scratch(&alias);
    scratch.populate().unwrap();
    let roots = super::read_roots(&command);
    for name in ["registry", "git"] {
        let target = std::fs::read_link(home.0.join(name)).unwrap();
        assert_eq!(target, alias.join(name));
        assert!(
            roots.contains(&target),
            "{} is not granted",
            target.display()
        );
    }
}
