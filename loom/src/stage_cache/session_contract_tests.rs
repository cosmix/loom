//! Contract tests for the stage cache in sessions: the wrapper's exports, the
//! session sandbox grant that replaces the host package caches, and the
//! removal of a stage's root, which never follows a symlink.

use crate::models::stage::Implementers;
use crate::plan::schema::{SandboxConfig, StageSandboxConfig, StageType};
use crate::sandbox::{generate_settings_json, merge_config};
use crate::stage_cache::cleanup::remove_stage_cache;
use crate::stage_cache::session::{apply_session_grant, wrapper_exports};
use crate::stage_cache::{Consumer, StageCache};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Host package caches a session must no longer be granted.
const HOST_CACHES: [&str; 5] = [
    "~/go/pkg",
    "~/.cache/go-build",
    "~/.npm",
    "~/.cache/uv",
    "~/.bun/install/cache",
];

/// Variables the session wrapper must export, each as `environment` gives it.
const SESSION_VARS: [&str; 6] = [
    "GOCACHE",
    "UV_CACHE_DIR",
    "npm_config_cache",
    "SCCACHE_SERVER_UDS",
    "CCACHE_BASEDIR",
    "GOPROXY",
];

/// A TempDir and its canonical path (macOS temp paths start at a symlink).
fn temp() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().canonicalize().unwrap();
    (dir, path)
}

/// A home short enough that the session's sccache socket fits `sun_path`.
fn short_home() -> tempfile::TempDir {
    tempfile::tempdir_in("/tmp")
        .or_else(|_| tempfile::tempdir())
        .unwrap()
}

/// What `/bin/sh` makes of `exports` as the continuation lines of an
/// `exec env -i` invocation: the variables the child process receives.
fn evaluated(exports: &str) -> BTreeMap<String, String> {
    let script = format!("exec /usr/bin/env -i \\\n{exports}    /usr/bin/env\n");
    let output = Command::new("/bin/sh").arg("-c").arg(&script).output();
    let output = output.unwrap();
    assert!(
        output.status.success(),
        "the exports do not run as shell: {}\n{script}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let pairs = stdout.lines().filter_map(|line| line.split_once('='));
    pairs.map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

#[test]
fn session_wrapper_exports_the_stage_cache() {
    let ((_r, repo), (_t, tree)) = (temp(), temp());
    let home_dir = short_home();
    let worktree = tree.join("work tree");
    std::fs::create_dir_all(&worktree).unwrap();
    let cache = StageCache::resolve(&repo, "alpha", home_dir.path(), None).unwrap();
    let host = BTreeMap::from([
        ("HOME".to_string(), OsString::from(home_dir.path())),
        (
            "GOPROXY".to_string(),
            OsString::from("https://proxy.example"),
        ),
    ]);
    let exports = wrapper_exports(&cache, &worktree, &host);
    assert!(
        exports.lines().all(|line| line.ends_with(" \\")),
        "{exports}"
    );
    let exported = evaluated(&exports);
    let session = cache.environment(Consumer::Session, &worktree, &host, None);
    for name in SESSION_VARS {
        let expected = session.get(name).map(|v| v.to_string_lossy().into_owned());
        assert!(expected.is_some(), "Session environment lacks {name}");
        assert_eq!(exported.get(name), expected.as_ref(), "{name}\n{exports}");
    }
    assert_eq!(exported["CCACHE_BASEDIR"], worktree.display().to_string());
    let proxy = &exported["GOPROXY"];
    assert!(proxy.ends_with("https://proxy.example"), "{proxy}");
    assert!(!proxy.ends_with(",off"), "HostJob GOPROXY: {proxy}");
    let goflags = exported.get("GOFLAGS").map(String::as_str);
    assert!(!goflags.unwrap_or("").contains("-count=1"), "{goflags:?}");
    assert!(exported["SCCACHE_SERVER_UDS"].ends_with("/s.sock"));
    assert!(!exports.contains("h.sock"), "HostJob socket: {exports}");
}

#[test]
fn sessions_no_longer_write_host_caches() {
    let ((_r, repo), (_h, home)) = (temp(), temp());
    let cache = StageCache::resolve(&repo, "alpha", &home, None).unwrap();
    let plan = SandboxConfig::default();
    let stage = StageSandboxConfig::default();
    let mut config = merge_config(&plan, &stage, StageType::Standard, &Implementers::default());
    apply_session_grant(&cache, &mut config);
    let settings = generate_settings_json(&config);
    let allow_write: Vec<&str> = settings["sandbox"]["filesystem"]["allowWrite"]
        .as_array()
        .expect("settings carry sandbox.filesystem.allowWrite")
        .iter()
        .filter_map(|value| value.as_str())
        .collect();
    let root = cache.root().display().to_string();
    assert!(cache.root().is_absolute(), "{root}");
    assert!(allow_write.contains(&root.as_str()), "{allow_write:?}");
    for host_cache in HOST_CACHES {
        assert!(
            !allow_write.contains(&host_cache),
            "still grants {host_cache}"
        );
    }
    assert!(
        allow_write.contains(&"~/.cargo/registry"),
        "cargo stays granted: {allow_write:?}"
    );
}

/// A directory outside every stage cache root holding `kept`.
fn outside_with_kept_file() -> (tempfile::TempDir, PathBuf) {
    let (dir, path) = temp();
    std::fs::write(path.join("kept"), "keep").unwrap();
    (dir, path)
}

fn assert_kept(outside: &Path) {
    let kept = outside.join("kept");
    assert_eq!(std::fs::read_to_string(&kept).ok().as_deref(), Some("keep"));
}

#[test]
fn stage_cache_removal_never_follows_a_symlink() {
    let ((_r, repo), (_h, home)) = (temp(), temp());
    let (_o, outside) = outside_with_kept_file();
    let cache = StageCache::resolve(&repo, "alpha", &home, None).unwrap();
    cache.ensure().unwrap();
    std::fs::create_dir_all(cache.root().join("go-build/nested")).unwrap();
    symlink(&outside, cache.root().join("link")).unwrap();
    symlink(&outside, cache.root().join("go-build/nested/link")).unwrap();
    assert!(
        remove_stage_cache(&cache).unwrap(),
        "a present root is removed"
    );
    assert!(
        cache.root().symlink_metadata().is_err(),
        "root still present"
    );
    assert_kept(&outside);
    assert!(
        !remove_stage_cache(&cache).unwrap(),
        "an absent root is false"
    );

    let (_l, linked) = outside_with_kept_file();
    let planted = StageCache::resolve(&repo, "beta", &home, None).unwrap();
    std::fs::create_dir_all(planted.root().parent().unwrap()).unwrap();
    symlink(&linked, planted.root()).unwrap();
    let removed = remove_stage_cache(&planted);
    assert!(removed.is_err(), "a symlinked root is refused: {removed:?}");
    assert_kept(&linked);
    assert!(linked.is_dir(), "the link target survives");
}
