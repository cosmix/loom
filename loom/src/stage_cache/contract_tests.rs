//! Contract tests for the per-stage build-cache root. The confined tests pin
//! the process `HOME` and `PATH` (hence `#[serial]`): the runner builds a
//! confined command's environment and read roots from loom's own.

use crate::models::stage::CommandConfinement;
use crate::process::sandbox_probe::skip_unless;
use crate::stage_cache::{seed_read_roots, toolchain_read_roots};
use crate::stage_cache::{Consumer, DeveloperTools, StageCache};
use crate::verify::criteria::{host_launcher, run_spec_in_stage, CommandSpec, CriterionResult};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Variables a pinned host environment drops, so seeds resolve from `HOME`.
const UNSET: &str = "GOMODCACHE GOPATH GRADLE_USER_HOME CCACHE_DIR XDG_CACHE_HOME MAVEN_OPTS";

/// Pins `HOME`, prepends to `PATH` and unsets [`UNSET`]; restores on drop.
struct HostEnvGuard(Vec<(&'static str, Option<OsString>)>);

impl HostEnvGuard {
    fn pin(home: &Path, path_prefix: &[PathBuf]) -> Self {
        let names = ["HOME", "PATH"].into_iter().chain(UNSET.split(' '));
        let saved = names.map(|name| (name, std::env::var_os(name))).collect();
        let host_path = std::env::var_os("PATH").unwrap_or_default();
        let mut entries = path_prefix.to_vec();
        entries.extend(std::env::split_paths(&host_path));
        std::env::set_var("PATH", std::env::join_paths(entries).unwrap());
        std::env::set_var("HOME", home);
        UNSET.split(' ').for_each(|name| std::env::remove_var(name));
        Self(saved)
    }
}

impl Drop for HostEnvGuard {
    fn drop(&mut self) {
        for (name, value) in &self.0 {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }
}

/// A TempDir and its canonical path (macOS temp paths start at a symlink).
fn temp() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().canonicalize().unwrap();
    (dir, path)
}

fn stage(repo: &Path, home: &Path) -> StageCache {
    StageCache::resolve(repo, "alpha", home, None).unwrap()
}

fn consumers() -> [(Consumer, &'static str); 3] {
    [
        (Consumer::HostJob, "h.sock"),
        (Consumer::Session, "s.sock"),
        (Consumer::Worker, "w.sock"),
    ]
}

fn host_env(home: &Path, extra: &[(&str, &str)]) -> BTreeMap<String, OsString> {
    let mut env = BTreeMap::from([("HOME".to_string(), OsString::from(home))]);
    for (name, value) in extra {
        env.insert(name.to_string(), OsString::from(value));
    }
    env
}

fn var(env: &BTreeMap<String, OsString>, name: &str) -> Option<String> {
    env.get(name).map(|v| v.to_string_lossy().into_owned())
}

fn shown(path: &Path) -> String {
    path.display().to_string()
}

fn sandbox_absent(test: &str) -> bool {
    skip_unless(
        host_launcher().is_some(),
        test,
        "OS sandbox launcher absent",
    )
}

fn run_confined(script: &str, tree: &Path, cache: &StageCache) -> CriterionResult {
    let spec = CommandSpec::shell(script);
    let timeout = Duration::from_secs(60);
    let confined = CommandConfinement::Confined;
    run_spec_in_stage(&spec, Some(tree), Some(tree), timeout, confined, cache).unwrap()
}

fn write_file(path: &Path, contents: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

#[test]
fn stage_root_is_stable_and_outside_the_tree() {
    let ((_r, repo), (_h, home), (_o, other)) = (temp(), temp(), temp());
    let resolve =
        |repo: &Path, id: &str, xdg: Option<&Path>| StageCache::resolve(repo, id, &home, xdg);
    let alpha = stage(&repo, &home);
    assert_eq!(alpha, stage(&repo, &home));
    let beta = resolve(&repo, "beta", None).unwrap();
    assert_ne!(alpha.root(), beta.root());
    assert_ne!(
        alpha.root(),
        stage(&other, &home).root(),
        "keyed on the repo"
    );
    assert!(!alpha.root().starts_with(&repo), "{:?}", alpha.root());
    assert!(!alpha.root().exists(), "resolve must not write");
    let base = if cfg!(target_os = "macos") {
        "Library/Caches"
    } else {
        ".cache"
    };
    let parent = home.join(base).join("loom/stage-cache");
    let long = resolve(&repo, &"s".repeat(300), None).unwrap();
    for cache in [&alpha, &beta, &long] {
        assert_eq!(cache.root().parent(), Some(parent.as_path()));
        let name = cache.root().file_name().unwrap().to_str().unwrap();
        assert_eq!(name.len(), 24, "{name}");
        let hex = |b: u8| b.is_ascii_digit() || (b'a'..=b'f').contains(&b);
        assert!(name.bytes().all(hex), "{name}");
    }
    let (_x, xdg) = temp();
    let in_xdg = resolve(&repo, "alpha", Some(&xdg)).unwrap();
    let xdg_parent = xdg.join("loom/stage-cache");
    assert_eq!(in_xdg.root().parent(), Some(xdg_parent.as_path()));
    assert_eq!(in_xdg.root().file_name(), alpha.root().file_name());
    let relative = resolve(&repo, "alpha", Some(Path::new("rel"))).unwrap();
    assert_eq!(relative, alpha, "a relative XDG_CACHE_HOME is ignored");
    for bad in ["", "a/b", "a\0b", ".."] {
        assert!(resolve(&repo, bad, None).is_err(), "{bad:?}");
    }
}

#[test]
#[serial_test::serial]
fn stage_env_wins_over_per_command_scratch() {
    if sandbox_absent("stage_env_wins_over_per_command_scratch") {
        return;
    }
    let ((_r, repo), (_h, home), (_t, tree)) = (temp(), temp(), temp());
    let cache = stage(&repo, &home);
    let expected = "GOCACHE=go-build XDG_CACHE_HOME=xdg UV_CACHE_DIR=uv \
        BUN_INSTALL_CACHE_DIR=bun npm_config_cache=npm CCACHE_DIR=ccache \
        GRADLE_USER_HOME=gradle SCCACHE_DIR=sccache";
    let pairs: Vec<(&str, &str)> = expected
        .split_whitespace()
        .map(|pair| pair.split_once('=').unwrap())
        .collect();
    let script: Vec<String> = pairs
        .iter()
        .map(|(name, _)| format!("printf '{name}=%s\\n' \"${name}\""))
        .collect();
    let result = run_confined(&script.join("; "), &tree, &cache);
    assert!(result.success, "{result:?}");
    for (name, child) in pairs {
        let line = format!("{name}={}", shown(&cache.root().join(child)));
        let found = result.stdout.lines().any(|l| l == line);
        assert!(found, "{line} missing from\n{}", result.stdout);
    }
}

#[test]
#[serial_test::serial]
fn stage_cache_persists_across_confined_commands() {
    if sandbox_absent("stage_cache_persists_across_confined_commands") {
        return;
    }
    let ((_r, repo), (_h, home), (_t, tree)) = (temp(), temp(), temp());
    let cache = stage(&repo, &home);
    let write = "mkdir -p \"$GOCACHE\" && printf persisted > \"$GOCACHE/probe\"";
    let first = run_confined(write, &tree, &cache);
    assert!(first.success, "the root must be writable inside: {first:?}");
    let probe = cache.root().join("go-build/probe");
    assert_eq!(std::fs::read_to_string(&probe).unwrap(), "persisted");
    let second = run_confined("cat \"$GOCACHE/probe\"", &tree, &cache.clone());
    assert!(second.success, "the root must survive commands: {second:?}");
    assert_eq!(second.stdout, "persisted");
}

#[test]
#[serial_test::serial]
fn host_caches_stay_read_only() {
    let ((_r, repo), (_h, home), (_t, tree)) = (temp(), temp(), temp());
    let seeds = [
        "go/pkg/mod/cache/download",
        ".m2/repository",
        ".gradle/caches",
    ];
    let seeds = seeds.map(|seed| home.join(seed));
    let dirs = [
        seeds[0].clone(),
        seeds[1].clone(),
        seeds[2].join("modules-2"),
    ];
    for dir in &dirs {
        write_file(&dir.join("sentinel"), "SENTINEL\n");
    }
    let granted = seed_read_roots(&host_env(&home, &[]));
    for seed in &seeds {
        assert!(granted.contains(seed), "{seed:?} missing from {granted:?}");
    }
    if sandbox_absent("host_caches_stay_read_only") {
        return;
    }
    let script: Vec<String> = dirs
        .iter()
        .map(|dir| {
            let dir = shown(dir);
            format!("cat '{dir}/sentinel' || exit 10; printf x > '{dir}/written' && echo WROTE")
        })
        .collect();
    let cache = stage(&repo, &home);
    let _env = HostEnvGuard::pin(&home, &[]);
    let result = run_confined(&format!("{}; echo DONE", script.join("; ")), &tree, &cache);
    assert!(result.success, "every seed must be readable: {result:?}");
    assert_eq!(result.stdout.matches("SENTINEL").count(), 3, "{result:?}");
    assert!(result.stdout.contains("DONE") && !result.stdout.contains("WROTE"));
    for dir in dirs {
        assert!(!dir.join("written").exists(), "{dir:?} was written");
    }
}

#[test]
fn host_seeds_are_named_in_the_environment() {
    let ((_r, repo), (_h, home), (_t, tree)) = (temp(), temp(), temp());
    let go_seed = home.join("go/pkg/mod/cache/download");
    let maven_seed = home.join(".m2/repository");
    let gradle_seed = home.join(".gradle/caches");
    for dir in [&go_seed, &maven_seed, &gradle_seed.join("modules-2")] {
        std::fs::create_dir_all(dir).unwrap();
    }
    let cache = stage(&repo, &home);
    let host = host_env(&home, &[("GOPROXY", "https://proxy.example")]);
    let seed_url = format!("file://{}", shown(&go_seed));
    let local = format!("-Dmaven.repo.local={}", shown(&cache.root().join("m2")));
    let tail = format!("-Dmaven.repo.local.tail={}", shown(&maven_seed));
    for (consumer, proxy) in [
        (Consumer::HostJob, "off"),
        (Consumer::Session, "https://proxy.example"),
    ] {
        let env = cache.environment(consumer, &tree, &host, None);
        let expected = format!("{seed_url},{proxy}");
        assert_eq!(var(&env, "GOPROXY"), Some(expected));
        let maven = var(&env, "MAVEN_OPTS").unwrap_or_default();
        assert!(maven.split(' ').any(|opt| opt == local), "{maven}");
        assert!(maven.split(' ').any(|opt| opt == tail), "{maven}");
        let gradle = var(&env, "GRADLE_RO_DEP_CACHE");
        assert_eq!(gradle, Some(shown(&gradle_seed)));
    }
}

#[test]
fn host_job_go_test_never_reuses_cached_results() {
    let ((_r, repo), (_h, home), (_t, tree)) = (temp(), temp(), temp());
    let cache = stage(&repo, &home);
    let goflags = |consumer: Consumer, host: Option<&str>| {
        let extra: Vec<(&str, &str)> = host.map(|value| ("GOFLAGS", value)).into_iter().collect();
        let env = cache.environment(consumer, &tree, &host_env(&home, &extra), None);
        var(&env, "GOFLAGS")
    };
    let host_job = goflags(Consumer::HostJob, Some("-mod=mod"));
    assert_eq!(host_job.as_deref(), Some("-mod=mod -count=1"));
    let bare = goflags(Consumer::HostJob, None);
    assert_eq!(bare.as_deref(), Some("-count=1"));
    let already = goflags(Consumer::HostJob, Some("-mod=mod -count=1"));
    assert_eq!(already.as_deref(), Some("-mod=mod -count=1"));
    let session = goflags(Consumer::Session, Some("-mod=mod"));
    let reusable = session.is_none() || session.as_deref() == Some("-mod=mod");
    assert!(reusable, "a session keeps the host GOFLAGS: {session:?}");
}

#[test]
#[serial_test::serial]
fn path_toolchains_are_readable() {
    use std::os::unix::fs::PermissionsExt;
    let ((_r, repo), (_h, home), (_t, tree)) = (temp(), temp(), temp());
    let install = home.join(".bun/install/global/node_modules/x/bin/faketool");
    let other = home.join("tools/bin/othertool");
    for (tool, word) in [(&install, "TOOL_OK"), (&other, "OTHER_OK")] {
        write_file(tool, &format!("#!/bin/sh\necho {word}\n"));
        std::fs::set_permissions(tool, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let local_bin = home.join(".local/bin");
    std::fs::create_dir_all(&local_bin).unwrap();
    std::os::unix::fs::symlink(&install, local_bin.join("faketool")).unwrap();
    let entries = [local_bin.clone(), home.join("tools/bin")];
    let path = std::env::join_paths(&entries).unwrap();
    let roots = toolchain_read_roots(&host_env(&home, &[("PATH", path.to_str().unwrap())]));
    for target in [&install, &other, &local_bin.join("faketool")] {
        let covered = roots
            .iter()
            .any(|root| target.starts_with(root) && root.starts_with(&home) && root != &home);
        assert!(covered, "{target:?} is not readable through {roots:?}");
    }
    if sandbox_absent("path_toolchains_are_readable") {
        return;
    }
    let cache = stage(&repo, &home);
    let _env = HostEnvGuard::pin(&home, &entries);
    let result = run_confined("faketool && othertool", &tree, &cache);
    assert!(result.success, "{result:?}");
    assert!(result.stdout.contains("TOOL_OK") && result.stdout.contains("OTHER_OK"));
}

#[test]
#[serial_test::serial]
fn credential_dirs_on_path_stay_hidden() {
    let ((_r, repo), (_h, home), (_t, tree)) = (temp(), temp(), temp());
    write_file(&home.join(".ssh/key"), "SECRET_KEY\n");
    std::fs::create_dir_all(home.join(".aws/bin")).unwrap();
    write_file(&home.join("tools/bin/marker"), "MARKER_OK\n");
    let entries = [".ssh", ".aws/bin", "tools/bin"].map(|entry| home.join(entry));
    let path = std::env::join_paths(&entries).unwrap();
    let roots = toolchain_read_roots(&host_env(&home, &[("PATH", path.to_str().unwrap())]));
    assert!(roots.contains(&entries[2]), "control missing: {roots:?}");
    for credential in [home.join(".ssh"), home.join(".aws")] {
        for root in &roots {
            let overlaps = root.starts_with(&credential) || credential.starts_with(root);
            assert!(!overlaps, "{root:?} exposes {credential:?}");
        }
    }
    if sandbox_absent("credential_dirs_on_path_stay_hidden") {
        return;
    }
    let cache = stage(&repo, &home);
    let _env = HostEnvGuard::pin(&home, &entries);
    let marker = shown(&entries[2].join("marker"));
    let key = shown(&home.join(".ssh/key"));
    let result = run_confined(&format!("cat '{marker}'; cat '{key}'"), &tree, &cache);
    assert!(result.stdout.contains("MARKER_OK"), "control: {result:?}");
    assert!(!result.stdout.contains("SECRET_KEY"), "{result:?}");
    assert!(!result.success, "reading the key must fail: {result:?}");
}

#[test]
fn developer_tools_bypass_the_xcrun_shims() {
    let ((_r, repo), (_h, home), (_t, tree)) = (temp(), temp(), temp());
    let ((_d, developer_dir), (_s, sdk_root)) = (temp(), temp());
    let toolchain = developer_dir.join("Toolchains/XcodeDefault.xctoolchain/usr/bin");
    let usr_bin = developer_dir.join("usr/bin");
    for dir in [&toolchain, &usr_bin] {
        std::fs::create_dir_all(dir).unwrap();
    }
    let developer = DeveloperTools {
        developer_dir: developer_dir.clone(),
        sdk_root: sdk_root.clone(),
    };
    let cache = stage(&repo, &home);
    let host = host_env(&home, &[("PATH", "/usr/bin:/bin")]);
    let path = format!("{}:{}:/usr/bin:/bin", shown(&toolchain), shown(&usr_bin));
    for (consumer, label) in consumers() {
        let env = cache.environment(consumer, &tree, &host, Some(&developer));
        assert_eq!(var(&env, "PATH").as_deref(), Some(path.as_str()), "{label}");
        assert_eq!(var(&env, "SDKROOT"), Some(shown(&sdk_root)), "{label}");
        let dir = var(&env, "DEVELOPER_DIR");
        assert_eq!(dir, Some(shown(&developer_dir)), "{label}");
    }
}

#[test]
fn host_job_sccache_socket_is_not_the_sessions() {
    let ((_r, repo), (_h, home), (_t, tree)) = (temp(), temp(), temp());
    let host = host_env(&home, &[]);
    let short = StageCache::resolve(&repo, "alpha", &home, Some(Path::new("/xdg"))).unwrap();
    let socket_dir = Path::new("/run/sd");
    let private = short.clone().with_socket_dir(socket_dir);
    for (consumer, socket) in consumers() {
        let env = private.environment(consumer, &tree, &host, None);
        let parent = if consumer == Consumer::HostJob {
            private.root()
        } else {
            socket_dir
        };
        let expected = shown(&parent.join(socket));
        assert_eq!(var(&env, "SCCACHE_SERVER_UDS"), Some(expected));
        let dir = shown(&short.root().join("sccache"));
        assert_eq!(var(&env, "SCCACHE_DIR"), Some(dir), "{socket}");
    }
    let long = stage(&repo, &home.join("h".repeat(200)));
    assert!(long.root().join("h.sock").as_os_str().len() > 108);
    let long_dir = Path::new("/run").join("d".repeat(120));
    let both = long.clone().with_socket_dir(&long_dir);
    for (consumer, socket) in consumers() {
        let env = both.environment(consumer, &tree, &host, None);
        let uds = var(&env, "SCCACHE_SERVER_UDS");
        assert_eq!(uds, None, "{socket} past SUN_LEN must be omitted");
    }
}
