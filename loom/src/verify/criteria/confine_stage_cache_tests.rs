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
use crate::verify::criteria::{
    run_spec_in_optional_tree, run_spec_in_stage, CommandSpec, CriterionResult,
};

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
        crate::process::sandbox_probe::confined_commands_runnable(),
        test,
        "confined commands cannot start",
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
    let roots = super::read_roots_beside(&command, &[tree.as_path()], None);
    assert!(roots.contains(&beside), "{roots:?}");
    for root in &roots {
        let overlaps = root.starts_with(&tree) || tree.starts_with(root);
        assert!(!overlaps, "{root:?} overlaps the tree: {roots:?}");
    }
    let installation = super::read_roots(&command);
    assert!(!installation.contains(&beside), "{installation:?}");
}

/// A symlink a command planted in a tree `PATH` entry grants the next command
/// nothing: the entry is never expanded into its links' target directories.
#[test]
fn a_planted_tree_symlink_grants_no_read_root() {
    let (_h, home) = temp();
    let tree = home.join("src/tree");
    let inside = tree.join("node_modules/.bin");
    let private = home.join("Documents/tax/bin");
    for dir in [&inside, &private] {
        std::fs::create_dir_all(dir).unwrap();
    }
    std::fs::write(private.join("2025.pdf"), "PRIVATE\n").unwrap();
    std::os::unix::fs::symlink(private.join("2025.pdf"), inside.join("x")).unwrap();
    let mut command = Command::new("/bin/sh");
    command.env("HOME", &home).env("PATH", &inside);
    let roots = super::read_roots_beside(&command, &[tree.as_path()], None);
    assert!(
        roots.iter().all(|root| !private.starts_with(root)),
        "{private:?} readable through {roots:?}"
    );
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
    let roots = super::read_roots_beside(&command, &[tree.as_path()], None);
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

/// Without a stage cache (a plan dry run) a confined command still resolves
/// the developer directory's tools and sees `SDKROOT`: through the shim every
/// `cc` of a cold build would start an `xcodebuild` lookup.
#[cfg(target_os = "macos")]
#[test]
#[serial_test::serial]
fn a_command_without_a_stage_cache_resolves_git_in_the_developer_directory() {
    let test = "a_command_without_a_stage_cache_resolves_git_in_the_developer_directory";
    let detected = DeveloperTools::detect();
    if skip_unless(detected.is_some(), test, "no macOS developer tools") || launcher_absent(test) {
        return;
    }
    let developer = detected.unwrap();
    let (_t, tree) = temp();
    let spec = CommandSpec::shell("command -v git && printf '%s\\n' \"$SDKROOT\"");
    let timeout = Duration::from_secs(60);
    let confined = CommandConfinement::Confined;
    let result =
        run_spec_in_optional_tree(&spec, Some(&tree), Some(&tree), timeout, confined).unwrap();
    assert!(result.success, "{result:?}");
    let mut lines = result.stdout.lines();
    let git = PathBuf::from(lines.next().unwrap_or_default());
    assert!(git.starts_with(&developer.developer_dir), "{result:?}");
    assert_eq!(
        lines.next().map(PathBuf::from),
        Some(developer.sdk_root),
        "{result:?}"
    );
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
    for name in ["settings.xml", "settings-security.xml"] {
        std::fs::write(home.join(".m2").join(name), "<settings/>\n").unwrap();
    }
    let _home = HomeGuard::pin(&home);
    let cache = StageCache::resolve(&repo, "alpha", &home, None).unwrap();
    let script = "cat \"$HOME/.m2/repository/seed\"; \
                  test -e \"$HOME/.m2/settings.xml\" && echo VISIBLE; \
                  test -e \"$HOME/.m2/settings-security.xml\" && echo SECURITY; echo DONE";
    let result = run_in_stage(script, &tree, &cache);
    assert!(result.stdout.contains("SEED"), "{result:?}");
    assert!(result.stdout.contains("DONE"), "{result:?}");
    assert!(!result.stdout.contains("VISIBLE"), "{result:?}");
    assert!(!result.stdout.contains("SECURITY"), "{result:?}");
}

/// The granted tree is its canonical parent joined with its own name: a tree
/// swapped for a symlink is refused, never resolved to the link's target.
#[test]
fn a_tree_that_is_a_symlink_is_refused() {
    let ((_b, base), (_t, target)) = (temp(), temp());
    let tree = base.join("tree");
    std::fs::create_dir(&tree).unwrap();
    assert_eq!(super::confined_tree(Some(&tree), &tree).unwrap(), tree);
    let link = base.join("link");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let error = super::confined_tree(Some(&link), &target).unwrap_err();
    assert!(format!("{error:#}").contains("not a symlink"), "{error:#}");
}

/// The stage cache root granted is the one `ensure` opened without following
/// links, under the canonical base, even when the base is reached through a
/// symlink.
#[test]
fn the_granted_root_is_built_from_the_canonical_base() {
    let ((_r, repo), (_h, home), (_x, real)) = (temp(), temp(), temp());
    let xdg = home.join("xdg-link");
    std::os::unix::fs::symlink(&real, &xdg).unwrap();
    let cache = StageCache::resolve(&repo, "alpha", &home, Some(&xdg)).unwrap();
    let granted = cache.ensure().unwrap();
    assert!(granted.starts_with(&real), "{granted:?}");
    assert_eq!(granted.file_name(), cache.root().file_name());
}

/// Sets one variable for the test's duration and restores it on drop.
struct VarGuard(&'static str, Option<OsString>);

impl VarGuard {
    fn set(name: &'static str, value: &Path) -> Self {
        let saved = std::env::var_os(name);
        std::env::set_var(name, value);
        Self(name, saved)
    }
}

impl Drop for VarGuard {
    fn drop(&mut self) {
        match &self.1 {
            Some(value) => std::env::set_var(self.0, value),
            None => std::env::remove_var(self.0),
        }
    }
}

/// A relocated Go seed (`GOMODCACHE` outside HOME) is readable in a host job,
/// and the `GOPROXY` the job gets names that same path.
#[test]
#[serial_test::serial]
fn a_relocated_seed_is_readable_and_named_by_the_same_path() {
    let ((_r, repo), (_h, home), (_g, gomod), (_t, tree)) = (temp(), temp(), temp(), temp());
    let seed = gomod.join("cache/download");
    std::fs::create_dir_all(&seed).unwrap();
    let _home = HomeGuard::pin(&home);
    let _gomod = VarGuard::set("GOMODCACHE", &gomod);
    let cache = StageCache::resolve(&repo, "alpha", &home, None).unwrap();
    let grants = super::Grants {
        git: None,
        cache_root: Some(Path::new("/unused")),
        memory_journals: None,
    };
    let seed_env = grants.seed_env().unwrap();
    let mut command = Command::new("/bin/sh");
    command.env("HOME", &home).env("PATH", "/usr/bin:/bin");
    let roots = super::read_roots_beside(&command, &[tree.as_path()], Some(&seed_env));
    assert!(roots.contains(&seed), "{roots:?}");
    let env = cache.environment(
        crate::stage_cache::Consumer::HostJob,
        &tree,
        &seed_env,
        None,
    );
    let proxy = env["GOPROXY"].to_string_lossy().into_owned();
    assert!(
        proxy.contains(&format!("file://{}", seed.display())),
        "{proxy}"
    );
}

/// A host job's variables name the canonical root `wrap` grants, even when the
/// cache base is reached through a symlink.
#[test]
#[serial_test::serial]
fn a_symlinked_cache_base_gives_variables_under_the_granted_root() {
    if launcher_absent("a_symlinked_cache_base_gives_variables_under_the_granted_root") {
        return;
    }
    let ((_r, repo), (_h, home), (_t, tree)) = (temp(), temp(), temp());
    let (_x, real) = short_temp();
    let (_y, links) = short_temp();
    let xdg = links.join("xdg-link");
    std::os::unix::fs::symlink(&real, &xdg).unwrap();
    let _home = HomeGuard::pin(&home);
    let cache = StageCache::resolve(&repo, "alpha", &home, Some(&xdg)).unwrap();
    let mut original = Command::new("/bin/sh");
    original.current_dir(&tree);
    let network = super::NetworkAccess::Denied;
    let (wrapped, _scratch, _git) =
        super::wrap(&original, Some(&tree), network, Some(&cache), None).unwrap();
    let granted = cache.ensure().unwrap();
    assert!(granted.starts_with(&real), "{granted:?}");
    let named = |name: &str| {
        let value = wrapped.get_envs().find(|(key, _)| *key == name);
        PathBuf::from(value.unwrap().1.unwrap())
    };
    assert_eq!(named("GOCACHE"), granted.join("go-build"));
    assert_eq!(named("SCCACHE_SERVER_UDS"), granted.join("h.sock"));
}
