//! Unit tests for the cases the contract tests leave open.

use super::{seed_read_roots, toolchain_read_roots, Consumer, StageCache};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

fn temp() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().canonicalize().unwrap();
    (dir, path)
}

fn host(home: &Path, extra: &[(&str, &str)]) -> BTreeMap<String, OsString> {
    let mut env = BTreeMap::from([("HOME".to_string(), OsString::from(home))]);
    for (name, value) in extra {
        env.insert(name.to_string(), OsString::from(value));
    }
    env
}

fn var(env: &BTreeMap<String, OsString>, name: &str) -> Option<String> {
    env.get(name).map(|v| v.to_string_lossy().into_owned())
}

#[test]
fn base_is_the_platform_cache_dir() {
    let ((_r, repo), (_h, home)) = (temp(), temp());
    let cache = StageCache::resolve(&repo, "alpha", &home, None).unwrap();
    let expected = if cfg!(target_os = "macos") {
        home.join("Library/Caches/loom/stage-cache")
    } else {
        home.join(".cache/loom/stage-cache")
    };
    assert_eq!(cache.root().parent(), Some(expected.as_path()));
}

#[test]
fn missing_repo_root_hashes_as_given() {
    let (_h, home) = temp();
    let missing = home.join("absent/repo");
    let first = StageCache::resolve(&missing, "alpha", &home, None).unwrap();
    let second = StageCache::resolve(&missing, "alpha", &home, None).unwrap();
    assert_eq!(first, second);
}

#[test]
fn goflags_count_is_not_duplicated() {
    let ((_r, repo), (_h, home), (_t, tree)) = (temp(), temp(), temp());
    let cache = StageCache::resolve(&repo, "alpha", &home, None).unwrap();
    let flags = |value: &str| {
        let env = cache.environment(
            Consumer::HostJob,
            &tree,
            &host(&home, &[("GOFLAGS", value)]),
            None,
        );
        var(&env, "GOFLAGS")
    };
    assert_eq!(
        flags("-count=1 -mod=mod").as_deref(),
        Some("-count=1 -mod=mod")
    );
    assert_eq!(flags("-count=10").as_deref(), Some("-count=10 -count=1"));
    assert_eq!(flags("").as_deref(), Some("-count=1"));
    let worker = cache.environment(Consumer::Worker, &tree, &host(&home, &[]), None);
    assert_eq!(var(&worker, "GOFLAGS"), None);
}

#[test]
fn maven_opts_append_to_the_host_value() {
    let ((_r, repo), (_h, home), (_t, tree)) = (temp(), temp(), temp());
    let cache = StageCache::resolve(&repo, "alpha", &home, None).unwrap();
    let env = cache.environment(
        Consumer::Session,
        &tree,
        &host(&home, &[("MAVEN_OPTS", "-Xmx1g")]),
        None,
    );
    let local = cache.root().join("m2");
    let expected = format!("-Xmx1g -Dmaven.repo.local={}", local.display());
    assert_eq!(var(&env, "MAVEN_OPTS"), Some(expected), "no seed, no tail");
}

#[test]
fn absent_seeds_are_not_named() {
    let ((_r, repo), (_h, home), (_t, tree)) = (temp(), temp(), temp());
    let cache = StageCache::resolve(&repo, "alpha", &home, None).unwrap();
    let env = cache.environment(Consumer::Session, &tree, &host(&home, &[]), None);
    assert_eq!(
        var(&env, "GOPROXY").as_deref(),
        Some("https://proxy.golang.org,direct")
    );
    for name in [
        "GRADLE_RO_DEP_CACHE",
        "CCACHE_REMOTE_STORAGE",
        "PATH",
        "SDKROOT",
    ] {
        assert_eq!(var(&env, name), None, "{name}");
    }
    let job = cache.environment(Consumer::HostJob, &tree, &host(&home, &[]), None);
    assert_eq!(var(&job, "GOPROXY").as_deref(), Some("off"));
    assert!(env.values().all(|value| !value.is_empty()));
    assert!(seed_read_roots(&host(&home, &[])).is_empty());
}

#[test]
fn ccache_seed_is_read_only_secondary_storage() {
    let ((_r, repo), (_h, home), (_t, tree), (_c, seed)) = (temp(), temp(), temp(), temp());
    let cache = StageCache::resolve(&repo, "alpha", &home, None).unwrap();
    let seed_text = seed.display().to_string();
    let env_in = host(&home, &[("CCACHE_DIR", seed_text.as_str())]);
    let env = cache.environment(Consumer::HostJob, &tree, &env_in, None);
    let storage = format!("file:{seed_text}|read-only");
    assert_eq!(var(&env, "CCACHE_REMOTE_STORAGE"), Some(storage.clone()));
    assert_eq!(var(&env, "CCACHE_SECONDARY_STORAGE"), Some(storage));
    assert_eq!(
        var(&env, "CCACHE_BASEDIR"),
        Some(tree.display().to_string())
    );
    assert_eq!(seed_read_roots(&env_in), vec![seed]);
}

#[test]
fn seed_that_exposes_home_is_dropped() {
    let (_h, home) = temp();
    let home_text = home.display().to_string();
    let roots = seed_read_roots(&host(&home, &[("CCACHE_DIR", home_text.as_str())]));
    assert!(roots.is_empty(), "{roots:?}");
}

#[test]
fn toolchain_roots_skip_home_and_paths_outside_it() {
    let ((_h, home), (_o, outside)) = (temp(), temp());
    let path = std::env::join_paths([home.clone(), outside.clone()]).unwrap();
    let roots = toolchain_read_roots(&host(&home, &[("PATH", path.to_str().unwrap())]));
    assert!(
        roots.iter().all(|root| root != &home && root != &outside),
        "{roots:?}"
    );
}

#[test]
fn ensure_creates_a_private_root_and_keeps_entries() {
    let ((_r, repo), (_h, home)) = (temp(), temp());
    let cache = StageCache::resolve(&repo, "alpha", &home, None).unwrap();
    cache.ensure().unwrap();
    let mode = std::fs::metadata(cache.root())
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o700);
    std::fs::write(cache.root().join("entry"), "kept").unwrap();
    std::fs::set_permissions(cache.root(), std::fs::Permissions::from_mode(0o755)).unwrap();
    cache.ensure().unwrap();
    assert!(cache.root().join("entry").exists());
    let mode = std::fs::metadata(cache.root())
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o700);
}

#[test]
fn ensure_refuses_a_planted_symlink() {
    let ((_r, repo), (_h, home), (_e, elsewhere)) = (temp(), temp(), temp());
    let cache = StageCache::resolve(&repo, "alpha", &home, None).unwrap();
    std::fs::create_dir_all(cache.root().parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&elsewhere, cache.root()).unwrap();
    assert!(cache.ensure().is_err(), "a symlinked root must be refused");
    let parent_cache = StageCache::resolve(&repo, "beta", &home, Some(&home.join("x"))).unwrap();
    std::fs::create_dir_all(home.join("x/loom")).unwrap();
    std::os::unix::fs::symlink(&elsewhere, home.join("x/loom/stage-cache")).unwrap();
    assert!(
        parent_cache.ensure().is_err(),
        "a symlinked parent must be refused"
    );
    assert!(std::fs::read_dir(&elsewhere).unwrap().next().is_none());
}
