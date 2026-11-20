//! `wrapper_exports` escaping and the session grant. The frozen contract tests
//! (`session_contract_tests.rs`) cover the exported variables themselves.

use super::*;
use std::os::unix::ffi::OsStringExt;
use std::process::Command;

fn cache_in(home: &Path) -> StageCache {
    StageCache::resolve(home, "alpha", home, None).unwrap()
}

/// The variables `/bin/sh` gives a child of an `exec env -i` line made of `exports`.
fn evaluated(exports: &str) -> BTreeMap<String, String> {
    let script = format!("exec /usr/bin/env -i \\\n{exports}    /usr/bin/env\n");
    let output = Command::new("/bin/sh").arg("-c").arg(&script).output();
    let stdout = String::from_utf8(output.unwrap().stdout).unwrap();
    let pairs = stdout.lines().filter_map(|line| line.split_once('='));
    pairs.map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

#[test]
fn a_value_with_shell_metacharacters_reaches_the_session_unchanged() {
    let home = tempfile::tempdir().unwrap();
    let cache = cache_in(home.path());
    let hostile = "-Dx='a b' $(touch pwned) `id` \"q\" ; & | *";
    let host = BTreeMap::from([("MAVEN_OPTS".to_string(), OsString::from(hostile))]);
    let worktree = home.path().join("it's a $tree");

    let exports = wrapper_exports(&cache, &worktree, &host);
    let exported = evaluated(&exports);

    assert!(exported["MAVEN_OPTS"].starts_with(hostile), "{exports}");
    assert_eq!(exported["CCACHE_BASEDIR"], worktree.display().to_string());
    assert!(!home.path().join("pwned").exists());
}

#[test]
fn a_value_that_is_not_utf8_is_left_out() {
    let home = tempfile::tempdir().unwrap();
    let cache = cache_in(home.path());
    let host = BTreeMap::from([("GOPROXY".to_string(), OsString::from_vec(vec![0xff, 0xfe]))]);

    let exports = wrapper_exports(&cache, home.path(), &host);

    assert!(!exports.contains("GOPROXY"), "{exports}");
    assert!(exports.contains("GOCACHE="), "{exports}");
}

#[test]
fn the_grant_names_the_root_once() {
    let home = tempfile::tempdir().unwrap();
    let cache = cache_in(home.path());
    let mut config = crate::sandbox::stage_sandbox(home.path(), &Default::default());
    let before = config.filesystem.allow_write.len();

    apply_session_grant(&cache, &mut config);
    apply_session_grant(&cache, &mut config);

    let root = cache.root().display().to_string();
    let granted = &config.filesystem.allow_write;
    assert_eq!(granted.len(), before + 1, "{granted:?}");
    assert_eq!(granted.iter().filter(|path| **path == root).count(), 1);
}

#[test]
#[serial_test::serial]
fn the_host_environment_holds_only_the_names_the_cache_reads() {
    let host = host_env_from_process();

    assert!(host
        .keys()
        .all(|name| HOST_ENV_NAMES.contains(&name.as_str())));
    assert_eq!(host.get("PATH"), std::env::var_os("PATH").as_ref());
}
