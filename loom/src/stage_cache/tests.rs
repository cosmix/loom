//! Unit tests for the cases the contract tests leave open.

use super::{seed_read_roots, toolchain_read_roots, Consumer, StageCache};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

pub(super) fn temp() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().canonicalize().unwrap();
    (dir, path)
}

pub(super) fn host(home: &Path, extra: &[(&str, &str)]) -> BTreeMap<String, OsString> {
    let mut env = BTreeMap::from([("HOME".to_string(), OsString::from(home))]);
    for (name, value) in extra {
        env.insert(name.to_string(), OsString::from(value));
    }
    env
}

pub(super) fn var(env: &BTreeMap<String, OsString>, name: &str) -> Option<String> {
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

/// A seed whose path would split a list entry (`GOPROXY`, ccache's remote
/// storage, `MAVEN_OPTS`) is left out of the environment and the read roots.
#[test]
fn a_seed_that_breaks_list_syntax_is_left_out() {
    let ((_r, repo), (_t, tree), (_b, base)) = (temp(), temp(), temp());
    for name in ["home with space", "home,comma", "home|bar"] {
        let home = base.join(name);
        for seed in ["go/pkg/mod/cache/download", ".m2/repository", "ccache"] {
            std::fs::create_dir_all(home.join(seed)).unwrap();
        }
        let ccache = home.join("ccache").display().to_string();
        let env_in = host(&home, &[("CCACHE_DIR", ccache.as_str())]);
        let cache = StageCache::resolve(&repo, "alpha", &home, None).unwrap();
        let env = cache.environment(Consumer::Session, &tree, &env_in, None);
        let proxy = var(&env, "GOPROXY");
        assert_eq!(proxy.as_deref(), Some("https://proxy.golang.org,direct"));
        assert_eq!(var(&env, "CCACHE_REMOTE_STORAGE"), None, "{name}");
        let maven = var(&env, "MAVEN_OPTS").unwrap();
        assert!(!maven.contains("maven.repo.local.tail"), "{maven}");
        assert!(seed_read_roots(&env_in, &[]).is_empty(), "{name}");
    }
}

/// Every consumer's sccache socket lives in the private directory it is
/// given, never in the shared root; without one, or past `sun_path`, the
/// variable is left out.
#[test]
fn session_and_worker_sockets_live_in_their_own_directory() {
    let ((_r, repo), (_h, home), (_t, tree)) = (temp(), temp(), temp());
    let host = host(&home, &[]);
    let shared = StageCache::resolve(&repo, "alpha", &home, Some(Path::new("/xdg"))).unwrap();
    let socket = |cache: &StageCache, consumer| {
        var(
            &cache.environment(consumer, &tree, &host, None),
            "SCCACHE_SERVER_UDS",
        )
    };
    for consumer in [Consumer::Session, Consumer::Worker, Consumer::HostJob] {
        assert_eq!(socket(&shared, consumer), None, "{consumer:?}");
    }
    let private = shared.clone().with_socket_dir(Path::new("/run/session-1"));
    let session = socket(&private, Consumer::Session);
    assert_eq!(session.as_deref(), Some("/run/session-1/s.sock"));
    let worker = socket(&private, Consumer::Worker);
    assert_eq!(worker.as_deref(), Some("/run/session-1/w.sock"));
    let host_job = socket(&private, Consumer::HostJob);
    assert_eq!(host_job.as_deref(), Some("/run/session-1/h.sock"));
    let long = shared.with_socket_dir(&Path::new("/run").join("d".repeat(120)));
    assert_eq!(socket(&long, Consumer::Session), None);
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
    assert!(seed_read_roots(&host(&home, &[]), &[]).is_empty());
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
    assert_eq!(seed_read_roots(&env_in, &[]), vec![seed]);
}

/// A session names seeds inside its own writable cache root; a confined
/// command there can swap the directory for a symlink to any host directory.
/// Such a seed grants nothing and is named in no variable.
#[test]
fn seed_inside_a_writable_root_linked_outside_grants_nothing() {
    let ((_r, repo), (_h, home), (_w, tree), (_o, outside)) = (temp(), temp(), temp(), temp());
    std::fs::create_dir_all(outside.join("cache/download")).unwrap();
    std::os::unix::fs::symlink(&outside, tree.join("ccache")).unwrap();
    std::os::unix::fs::symlink(&outside, tree.join("go-mod")).unwrap();
    let (ccache, go_mod) = (tree.join("ccache"), tree.join("go-mod"));
    let env_in = host(
        &home,
        &[
            ("CCACHE_DIR", ccache.to_str().unwrap()),
            ("GOMODCACHE", go_mod.to_str().unwrap()),
        ],
    );
    let unconfined = seed_read_roots(&env_in, &[]);
    assert_eq!(unconfined, vec![outside.join("cache/download"), outside]);
    assert_eq!(
        seed_read_roots(&env_in, &[tree.as_path()]),
        Vec::<PathBuf>::new()
    );
    let cache = StageCache::resolve(&repo, "alpha", &home, None).unwrap();
    let env = cache.environment(Consumer::HostJob, &tree, &env_in, None);
    assert_eq!(var(&env, "CCACHE_REMOTE_STORAGE"), None);
    assert_eq!(var(&env, "GOPROXY").as_deref(), Some("off"));
}

#[test]
fn seed_that_exposes_home_is_dropped() {
    let (_h, home) = temp();
    let home_text = home.display().to_string();
    let roots = seed_read_roots(&host(&home, &[("CCACHE_DIR", home_text.as_str())]), &[]);
    assert!(roots.is_empty(), "{roots:?}");
}

/// A system install root is refused when it holds HOME or a credential
/// directory, as a root below HOME is.
#[test]
fn system_roots_holding_home_or_a_credential_are_left_out() {
    let ((_s, holds_home), (_c, holds_key), (_p, plain), (_o, other)) =
        (temp(), temp(), temp(), temp());
    let home = holds_home.join("users/me");
    std::fs::create_dir_all(&home).unwrap();
    let other_home = other.join("me");
    std::fs::create_dir_all(holds_key.join("keys")).unwrap();
    std::fs::create_dir_all(&other_home).unwrap();
    std::os::unix::fs::symlink(holds_key.join("keys"), other_home.join(".ssh")).unwrap();
    let systems = [holds_home.as_path(), holds_key.as_path(), plain.as_path()];
    let roots = super::roots::toolchain_read_roots_in(&host(&home, &[]), &systems, &[]);
    assert_eq!(roots, vec![holds_key.clone(), plain.clone()], "HOME inside");
    let roots = super::roots::toolchain_read_roots_in(&host(&other_home, &[]), &systems, &[]);
    assert_eq!(roots, vec![holds_home, plain], "a credential inside");
}

#[test]
fn toolchain_roots_skip_home_and_paths_outside_it() {
    let ((_h, home), (_o, outside)) = (temp(), temp());
    let path = std::env::join_paths([home.clone(), outside.clone()]).unwrap();
    let roots = toolchain_read_roots(&host(&home, &[("PATH", path.to_str().unwrap())]), &[]);
    assert!(
        roots.iter().all(|root| root != &home && root != &outside),
        "{roots:?}"
    );
}

#[test]
fn ensure_creates_a_private_root_and_keeps_entries() {
    let ((_r, repo), (_h, home)) = (temp(), temp());
    let cache = StageCache::resolve(&repo, "alpha", &home, None).unwrap();
    let canonical = cache.ensure().unwrap();
    assert_eq!(canonical, cache.root().canonicalize().unwrap());
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

/// `<socket dir>/h.sock` is emitted at `SUN_LEN - 1` bytes and omitted at
/// `SUN_LEN`.
#[test]
fn the_sccache_socket_is_emitted_below_sun_len_and_omitted_at_it() {
    const SUN_LEN: usize = if cfg!(target_os = "macos") { 104 } else { 108 };
    let ((_r, repo), (_h, home), (_t, tree)) = (temp(), temp(), temp());
    let fixed = "/h.sock".len();
    let cache = StageCache::resolve(&repo, "alpha", &home, None).unwrap();
    let socket_len = |dir_len: usize| {
        let dir = PathBuf::from(format!("/{}", "x".repeat(dir_len - 1)));
        let cache = cache.clone().with_socket_dir(&dir);
        let env = cache.environment(Consumer::HostJob, &tree, &host(&home, &[]), None);
        (
            dir.join("h.sock").as_os_str().len(),
            var(&env, "SCCACHE_SERVER_UDS"),
        )
    };
    let (len, socket) = socket_len(SUN_LEN - 1 - fixed);
    assert_eq!(len, SUN_LEN - 1);
    assert!(socket.is_some(), "{len} bytes fit sun_path");
    let (len, socket) = socket_len(SUN_LEN - fixed);
    assert_eq!(len, SUN_LEN);
    assert_eq!(socket, None, "{len} bytes leave no room for the NUL");
}

/// Every `ROOT_DIRS` variable is `<root>/<child>` for every consumer.
#[test]
fn every_root_dir_variable_names_a_child_of_the_root() {
    let ((_r, repo), (_h, home), (_t, tree)) = (temp(), temp(), temp());
    let cache = StageCache::resolve(&repo, "alpha", &home, None).unwrap();
    for consumer in [Consumer::Session, Consumer::Worker, Consumer::HostJob] {
        let env = cache.environment(consumer, &tree, &host(&home, &[]), None);
        for (name, child) in super::environment::ROOT_DIRS {
            let expected = cache.root().join(child).display().to_string();
            assert_eq!(var(&env, name), Some(expected), "{consumer:?} {name}");
        }
    }
}

#[test]
fn sccache_sockets_are_distinct_and_each_is_omitted_on_its_own_long_path() {
    let ((_r, repo), (_h, home), (_t, tree)) = (temp(), temp(), temp());
    let uds = |cache: &StageCache, consumer| {
        let env = cache.environment(consumer, &tree, &host(&home, &[]), None);
        var(&env, "SCCACHE_SERVER_UDS")
    };
    let short = StageCache::resolve(&repo, "alpha", &home, Some(Path::new("/xdg"))).unwrap();
    let socket_dir = Path::new("/run/sd");
    let private = short.clone().with_socket_dir(socket_dir);
    let mut seen = BTreeSet::new();
    for (consumer, path) in [
        (Consumer::HostJob, socket_dir.join("h.sock")),
        (Consumer::Session, socket_dir.join("s.sock")),
        (Consumer::Worker, socket_dir.join("w.sock")),
    ] {
        let expected = path.display().to_string();
        assert_eq!(uds(&private, consumer), Some(expected.clone()));
        seen.insert(expected);
    }
    assert_eq!(seen.len(), 3, "the consumers' sockets are distinct");
    let long = StageCache::resolve(&repo, "alpha", &home.join("h".repeat(200)), None).unwrap();
    let long_root = long.with_socket_dir(socket_dir);
    assert!(uds(&long_root, Consumer::HostJob).is_some(), "a long root");
    let long_dir = Path::new("/run").join("d".repeat(120));
    let long_socket_dir = short.clone().with_socket_dir(&long_dir);
    for consumer in [Consumer::Session, Consumer::Worker, Consumer::HostJob] {
        assert_eq!(uds(&long_socket_dir, consumer), None, "long dir");
        assert_eq!(uds(&short, consumer), None, "no socket directory");
    }
}
