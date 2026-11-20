//! `wrapper_exports` escaping and the session grant. The frozen contract tests
//! (`session_contract_tests.rs`) cover the exported variables themselves.

use super::*;
use std::os::unix::ffi::OsStringExt;
use std::process::Command;

use crate::stage_cache::environment::HOST_ENV_NAMES;
use crate::stage_cache::host_env_from_process;

fn cache_in(home: &Path) -> StageCache {
    StageCache::resolve(home, "alpha", home, None).unwrap()
}

/// The variables `/bin/sh`, started in `cwd`, gives a child of an
/// `exec env -i` line made of `exports`.
fn evaluated(exports: &str, cwd: &Path) -> BTreeMap<String, String> {
    let script = format!("exec /usr/bin/env -i \\\n{exports}    /usr/bin/env\n");
    let output = Command::new("/bin/sh")
        .arg("-c")
        .arg(&script)
        .current_dir(cwd)
        .output();
    let stdout = String::from_utf8(output.unwrap().stdout).unwrap();
    let pairs = stdout.lines().filter_map(|line| line.split_once('='));
    pairs.map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

/// The shell runs in `home`, so a command substitution that ran would leave
/// `pwned` there.
#[test]
fn a_value_with_shell_metacharacters_reaches_the_session_unchanged() {
    let home = tempfile::tempdir().unwrap();
    let cache = cache_in(home.path());
    let hostile = "-Dx='a b' $(touch pwned) `id` \"q\" ; & | *";
    let host = BTreeMap::from([("GOFLAGS".to_string(), OsString::from(hostile))]);
    let worktree = home
        .path()
        .join("it's a $tree $(touch pwned) `touch pwned` \"q\" ; & | *");

    let exports = wrapper_exports(&cache, &worktree, &host);
    let exported = evaluated(&exports, home.path());

    assert!(exported["GOFLAGS"].starts_with(hostile), "{exports}");
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
fn a_worktree_that_is_not_utf8_is_left_out() {
    let home = tempfile::tempdir().unwrap();
    let cache = cache_in(home.path());
    let worktree = home.path().join(OsString::from_vec(vec![0xff, 0xfe]));

    let exports = wrapper_exports(&cache, &worktree, &BTreeMap::new());

    assert!(!exports.contains("CCACHE_BASEDIR"), "{exports}");
    assert!(exports.contains("GOCACHE="), "{exports}");
}

/// The session's sccache socket is in its own scratch directory, which no
/// host job or worker is granted, never in the shared stage root.
#[test]
fn the_session_socket_is_in_its_scratch_directory() {
    // A host job's TMPDIR is longer than `sun_path`, which turns the socket off.
    let short = tempfile::tempdir_in("/tmp");
    if crate::process::sandbox_probe::skip_unless(
        short.is_ok(),
        "the_session_socket_is_in_its_scratch_directory",
        "/tmp is not writable, so no socket path fits sun_path",
    ) {
        return;
    }
    let home = short.unwrap();
    let scratch = home.path().join("scratch/session-1");
    let cache = cache_in(home.path()).with_socket_dir(&scratch);

    let exports = wrapper_exports(&cache, home.path(), &BTreeMap::new());
    let exported = evaluated(&exports, home.path());

    let socket = scratch.join("s.sock").display().to_string();
    assert!(socket.len() < 104, "{socket} does not fit sun_path");
    assert_eq!(
        exported.get("SCCACHE_SERVER_UDS"),
        Some(&socket),
        "{exports}"
    );
    let root = cache.root().display().to_string();
    assert!(!exported
        .values()
        .any(|value| value.ends_with(".sock") && value.starts_with(&root)));
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
