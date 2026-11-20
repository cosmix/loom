//! The stage cache inside the boundary: the roots it adds stay clear of the
//! writable tree, the real developer tools resolve before the xcrun shims, and
//! on macOS the host-job sccache socket is the only one a command may use and
//! Maven's settings stay hidden. The confined tests pin the process `HOME`
//! (hence `#[serial]`): a confined command's environment comes from loom's own.
// Only the overlap test runs off macOS; the confined helpers serve the rest.
#![cfg_attr(not(target_os = "macos"), allow(dead_code, unused_imports))]

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::models::stage::CommandConfinement;
use crate::process::sandbox_probe::skip_unless;
use crate::stage_cache::{DeveloperTools, StageCache};
use crate::verify::criteria::{run_spec_in_stage, CommandSpec, CriterionResult};

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

fn temp() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().canonicalize().unwrap();
    (dir, path)
}

/// A temporary cache base under `/tmp`: a host job's `TMPDIR` is long enough
/// to push `<root>/h.sock` past the socket address limit, which drops
/// `SCCACHE_SERVER_UDS` from the environment.
fn short_temp() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir_in("/tmp").unwrap();
    let path = dir.path().canonicalize().unwrap();
    (dir, path)
}

fn launcher_absent(test: &str) -> bool {
    skip_unless(
        super::launcher().is_some(),
        test,
        "OS sandbox launcher absent",
    )
}

fn run_in_stage(script: &str, tree: &Path, cache: &StageCache) -> CriterionResult {
    let spec = CommandSpec::shell(script);
    let timeout = Duration::from_secs(60);
    let confined = CommandConfinement::Confined;
    run_spec_in_stage(&spec, Some(tree), Some(tree), timeout, confined, cache).unwrap()
}

/// A `PATH` entry inside the tree is not made read-only over the writable
/// tree, nor is one containing it; one beside it stays readable.
#[test]
fn host_roots_overlapping_a_writable_root_are_left_out() {
    let (_h, home) = temp();
    let tree = home.join("src/tree");
    let inside = tree.join("node_modules/.bin");
    let beside = home.join("tools/bin");
    for dir in [&inside, &beside] {
        std::fs::create_dir_all(dir).unwrap();
    }
    let path = std::env::join_paths([&inside, &home.join("src"), &beside]).unwrap();
    let mut command = Command::new("/bin/sh");
    command.env("HOME", &home).env("PATH", path);
    let roots = super::read_roots_beside(&command, &[tree.as_path()]);
    assert!(roots.contains(&beside), "{roots:?}");
    for root in &roots {
        let overlaps = root.starts_with(&tree) || tree.starts_with(root);
        assert!(!overlaps, "{root:?} overlaps the tree: {roots:?}");
    }
    let installation = super::read_roots(&command);
    assert!(!installation.contains(&beside), "{installation:?}");
}

/// The running loom's directory, first on a confined `PATH`, does not become a
/// toolchain root below HOME: only the `LOOM_BIN` binary stays readable.
#[test]
fn the_loom_executable_directory_is_not_a_toolchain_root() {
    let (_h, home) = temp();
    let release = home.join("src/project/target/release");
    let beside = home.join("tools/bin");
    for dir in [&release, &beside] {
        std::fs::create_dir_all(dir).unwrap();
    }
    let loom = release.join("loom");
    std::fs::write(&loom, "").unwrap();
    let path = std::env::join_paths([&release, &beside]).unwrap();
    let mut command = Command::new("/bin/sh");
    command
        .env("HOME", &home)
        .env("PATH", path)
        .env("LOOM_BIN", &loom);
    let (_t, tree) = temp();
    let roots = super::read_roots_beside(&command, &[tree.as_path()]);
    assert!(roots.contains(&beside), "{roots:?}");
    assert!(roots.contains(&loom), "{roots:?}");
    assert!(!roots.contains(&release), "{roots:?}");
}

/// A criterion's bare `git` resolves to the developer directory's binary,
/// which needs no xcrun cache, not to the `/usr/bin` shim.
#[cfg(target_os = "macos")]
#[test]
#[serial_test::serial]
fn a_stage_command_resolves_git_in_the_developer_directory() {
    let test = "a_stage_command_resolves_git_in_the_developer_directory";
    let detected = DeveloperTools::detect();
    if skip_unless(detected.is_some(), test, "no macOS developer tools") || launcher_absent(test) {
        return;
    }
    let developer = detected.unwrap();
    let ((_r, repo), (_t, tree)) = (temp(), temp());
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let (_x, xdg) = temp();
    let cache = StageCache::resolve(&repo, "alpha", &home, Some(&xdg)).unwrap();
    let result = run_in_stage("command -v git", &tree, &cache);
    assert!(result.success, "{result:?}");
    let git = PathBuf::from(result.stdout.trim());
    assert!(git.starts_with(&developer.developer_dir), "{result:?}");
}

/// The host-job socket in the stage root can be bound and reached; any other
/// socket path in the root, the session's included, cannot be bound.
#[cfg(target_os = "macos")]
#[test]
#[serial_test::serial]
fn only_the_host_job_sccache_socket_is_granted() {
    if launcher_absent("only_the_host_job_sccache_socket_is_granted") {
        return;
    }
    let ((_r, repo), (_h, home), (_t, tree)) = (temp(), temp(), temp());
    let (_x, xdg) = short_temp();
    let _home = HomeGuard::pin(&home);
    let cache = StageCache::resolve(&repo, "alpha", &home, Some(&xdg)).unwrap();
    let socket = cache.root().join("h.sock");
    assert!(socket.as_os_str().len() < 104, "{socket:?} exceeds SUN_LEN");
    // Listen, then connect from the same process: no server to time.
    let probe = "use IO::Socket::UNIX; my $l = IO::Socket::UNIX->new(Type => SOCK_STREAM(), \
                 Local => $ARGV[0], Listen => 1) or die \"bind: $!\"; \
                 IO::Socket::UNIX->new(Type => SOCK_STREAM(), Peer => $ARGV[0]) \
                 or die \"connect: $!\"; print \"OK\\n\"";
    let script = format!(
        "/usr/bin/perl -e '{probe}' \"$SCCACHE_SERVER_UDS\" && \
         /usr/bin/perl -e '{probe}' \"$(dirname \"$SCCACHE_SERVER_UDS\")/s.sock\""
    );
    let result = run_in_stage(&script, &tree, &cache);
    assert_eq!(result.stdout.matches("OK").count(), 1, "{result:?}");
    assert!(result.stderr.contains("bind:"), "{result:?}");
    assert!(cache.root().join("h.sock").exists(), "{result:?}");
}

/// Maven's settings files are absent inside the boundary, metadata included,
/// while its local repository seed stays readable.
#[cfg(target_os = "macos")]
#[test]
#[serial_test::serial]
fn maven_settings_are_hidden_inside_the_boundary() {
    if launcher_absent("maven_settings_are_hidden_inside_the_boundary") {
        return;
    }
    let ((_r, repo), (_h, home), (_t, tree)) = (temp(), temp(), temp());
    std::fs::create_dir_all(home.join(".m2/repository")).unwrap();
    std::fs::write(home.join(".m2/repository/seed"), "SEED\n").unwrap();
    std::fs::write(home.join(".m2/settings.xml"), "<settings/>\n").unwrap();
    let _home = HomeGuard::pin(&home);
    let cache = StageCache::resolve(&repo, "alpha", &home, None).unwrap();
    let script = "cat \"$HOME/.m2/repository/seed\"; \
                  test -e \"$HOME/.m2/settings.xml\" && echo VISIBLE; echo DONE";
    let result = run_in_stage(script, &tree, &cache);
    assert!(result.stdout.contains("SEED"), "{result:?}");
    assert!(result.stdout.contains("DONE"), "{result:?}");
    assert!(!result.stdout.contains("VISIBLE"), "{result:?}");
}
