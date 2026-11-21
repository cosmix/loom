//! The boundary's environment, built from scratch, and the command the Cargo
//! scratch reads it from.
#[cfg(target_os = "linux")]
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
#[cfg(target_os = "linux")]
use std::process::Command;

#[cfg(target_os = "linux")]
use super::BOUNDARY_TMPDIR;
#[cfg(target_os = "linux")]
use crate::stage_cache::{host_env_from, Consumer, StageCache};

/// Where the launcher is mounted inside the boundary.
#[cfg(target_os = "linux")]
pub(super) const LOOM_BIN: &str = "/loom-bin/loom";

pub(super) fn lookup<'a>(host: &'a [(OsString, OsString)], key: &str) -> Option<&'a OsStr> {
    host.iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_os_str())
}

/// The boundary's whole environment, built from scratch: nothing of `host`
/// is inherited except `HOME`, `RUSTUP_HOME` and `CARGO_HOME`. `launch` marks
/// the real boundary; `codex_path` is the package's helper directory, set
/// whenever the boundary runs a codex package.
#[cfg(target_os = "linux")]
pub(super) fn environment(
    host: &[(OsString, OsString)],
    launch: bool,
    codex_path: Option<&Path>,
) -> Vec<(OsString, OsString)> {
    let mut path: Vec<PathBuf> = Vec::new();
    if launch {
        path.push("/loom-bin".into());
    }
    path.extend(codex_path.map(Path::to_path_buf));
    let mut env = vec![(
        "PATH".into(),
        search_path(path, host, &["/usr/local/bin", "/usr/bin", "/bin"]),
    )];
    env.extend(lookup(host, "HOME").map(|home| ("HOME".into(), home.to_os_string())));
    env.extend(fixed(Path::new(BOUNDARY_TMPDIR)));
    if launch {
        env.push(("LOOM_BIN".into(), LOOM_BIN.into()));
    }
    env.extend(toolchain_homes(host));
    env
}

/// The macOS boundary's whole environment, built from scratch as on Linux:
/// of `host` only `HOME`, `RUSTUP_HOME` and `CARGO_HOME`. `loom` is the
/// canonical running binary (`LOOM_BIN`, its directory first on `PATH`);
/// `tmp` and `codex_home` are the private invocation root's; the marker
/// names `invocation`. The bridge adds the proxy variables for its port.
#[cfg(target_os = "macos")]
pub(super) fn seatbelt_environment(
    host: &[(OsString, OsString)],
    loom: &Path,
    private: (&Path, &Path),
    invocation: &str,
    codex_path: Option<&Path>,
) -> Vec<(OsString, OsString)> {
    let (tmp, codex_home) = private;
    let mut path: Vec<PathBuf> = loom.parent().map(Path::to_path_buf).into_iter().collect();
    path.extend(codex_path.map(Path::to_path_buf));
    let system = [
        "/opt/homebrew/bin",
        "/usr/local/bin",
        "/usr/bin",
        "/bin",
        "/usr/sbin",
        "/sbin",
    ];
    let mut env = vec![("PATH".into(), search_path(path, host, &system))];
    env.extend(lookup(host, "HOME").map(|home| ("HOME".into(), home.to_os_string())));
    env.extend(fixed(tmp));
    env.push(("LOOM_BIN".into(), loom.into()));
    env.push(("CODEX_HOME".into(), codex_home.into()));
    let marker = super::quiescence_scan::BOUNDARY_MARKER_VAR;
    env.push((marker.into(), invocation.into()));
    env.extend(toolchain_homes(host));
    env
}

/// `first`, the host Cargo home's `bin` (workers compile: the toolchain
/// proxies live there) and `system`, as one `PATH`. An entry holding the
/// separator cannot be expressed in PATH and is dropped.
fn search_path(first: Vec<PathBuf>, host: &[(OsString, OsString)], system: &[&str]) -> OsString {
    let path = first
        .into_iter()
        .chain(host_cargo_home(host).map(|cargo| cargo.join("bin")))
        .chain(system.iter().map(PathBuf::from))
        .filter(|entry| !entry.as_os_str().as_encoded_bytes().contains(&b':'));
    std::env::join_paths(path).unwrap_or_else(|_| "/usr/bin:/bin".into())
}

/// The fixed variables of every boundary, its temporary directory `tmp`.
fn fixed(tmp: &Path) -> Vec<(OsString, OsString)> {
    let mut env: Vec<(OsString, OsString)> = [
        ("LANG", "C.UTF-8"),
        ("TERM", "dumb"),
        ("GIT_CONFIG_GLOBAL", "/dev/null"),
        ("LOOM_CODEX_FORWARD", "1"),
    ]
    .map(|(key, value)| (key.into(), value.into()))
    .into();
    for key in ["TMPDIR", "TMP", "TEMP"] {
        env.push((key.into(), tmp.into()));
    }
    env
}

/// `RUSTUP_HOME` and `CARGO_HOME` of `host`, where set.
fn toolchain_homes(host: &[(OsString, OsString)]) -> Vec<(OsString, OsString)> {
    ["RUSTUP_HOME", "CARGO_HOME"]
        .into_iter()
        .filter_map(|key| Some((key.into(), lookup(host, key)?.to_os_string())))
        .collect()
}

/// The host Cargo home: `CARGO_HOME`, else `$HOME/.cargo`.
pub(super) fn host_cargo_home(host: &[(OsString, OsString)]) -> Option<PathBuf> {
    lookup(host, "CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| lookup(host, "HOME").map(|home| Path::new(home).join(".cargo")))
}

/// The environment `CargoScratch::prepare` reads: `env` without `HOME`, the
/// host Cargo home named. Without `HOME` it probes no uv or bun cache for an
/// overlay; the boundary mounts none (its `TMPDIR` is private and no cache
/// variable points into the scratch), so that probe would be wasted work.
#[cfg(target_os = "linux")]
pub(super) fn scratch_probe(
    program: &OsStr,
    env: &[(OsString, OsString)],
    host: &[(OsString, OsString)],
) -> Command {
    let mut probe = Command::new(program);
    let kept = env.iter().filter(|(key, _)| key != "HOME");
    probe.env_clear().envs(kept.map(|(k, v)| (k, v)));
    if let Some(cargo) = host_cargo_home(host) {
        probe.env("CARGO_HOME", cargo);
    }
    probe
}

/// `env` by name, as the host environment the stage cache reads its seeds
/// from; a name that is not UTF-8 is skipped.
#[cfg(target_os = "linux")]
pub(super) fn host_map(env: &[(OsString, OsString)]) -> BTreeMap<String, OsString> {
    env.iter()
        .filter_map(|(name, value)| Some((name.to_str()?.to_string(), value.clone())))
        .collect()
}

/// `env` with the worker's stage cache variables set over it, each replacing
/// a variable of the same name. Seeds resolve from `host`, loom's own
/// environment, as every consumer's do. The worker's sccache socket is in
/// [`BOUNDARY_TMPDIR`], on this boundary's private `/tmp`, which no host job
/// or session can reach.
#[cfg(target_os = "linux")]
pub(super) fn with_stage_cache(
    env: &[(OsString, OsString)],
    host: &[(OsString, OsString)],
    cache: &StageCache,
    worktree: &Path,
) -> Vec<(OsString, OsString)> {
    let cache = cache.clone().with_socket_dir(Path::new(BOUNDARY_TMPDIR));
    let seeds = host_env_from(host.iter().cloned());
    let vars = cache.environment(Consumer::Worker, worktree, &seeds, None);
    let mut merged: Vec<_> = env
        .iter()
        .filter(|(name, _)| name.to_str().is_none_or(|name| !vars.contains_key(name)))
        .cloned()
        .collect();
    merged.extend(vars.into_iter().map(|(name, value)| (name.into(), value)));
    merged
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn the_worker_cache_replaces_same_named_variables_and_keeps_the_rest() {
        let home = tempfile::tempdir().unwrap();
        let cache = StageCache::resolve(home.path(), "alpha", home.path(), None).unwrap();
        let host: [(OsString, OsString); 1] = [("HOME".into(), home.path().into())];
        let env = environment(&host, false, None);

        let merged = with_stage_cache(&env, &host, &cache, home.path());

        let value = |name: &str| lookup(&merged, name).map(Path::new);
        assert_eq!(
            value("GOCACHE"),
            Some(cache.root().join("go-build").as_path())
        );
        assert_eq!(value("HOME"), Some(home.path()));
        assert_eq!(value("TMPDIR"), Some(Path::new(BOUNDARY_TMPDIR)));
        let socket = Path::new(BOUNDARY_TMPDIR).join("w.sock");
        assert_eq!(value("SCCACHE_SERVER_UDS"), Some(socket.as_path()));
        assert_eq!(
            merged.iter().filter(|(name, _)| name == "GOCACHE").count(),
            1
        );
    }
}

#[cfg(all(test, target_os = "macos"))]
mod macos_tests {
    use super::*;

    #[test]
    fn the_seatbelt_environment_is_clean_and_names_the_private_root() {
        let host: Vec<(OsString, OsString)> = [
            ("HOME", "/Users/u"),
            ("OPENAI_API_KEY", "fixture-key"),
            ("SSH_AUTH_SOCK", "/tmp/agent.sock"),
            ("CARGO_HOME", "/Users/u/.cargo"),
        ]
        .map(|(key, value)| (key.into(), value.into()))
        .into();
        let loom = Path::new("/opt/loom/bin/loom");
        let private = (
            Path::new("/c/native/inv/tmp"),
            Path::new("/c/native/inv/codex-home"),
        );
        let env = seatbelt_environment(&host, loom, private, "inv-1", None);
        let value = |name: &str| lookup(&env, name).map(|value| value.to_os_string());
        for (name, expected) in [
            ("LOOM_BIN", "/opt/loom/bin/loom"),
            ("TMPDIR", "/c/native/inv/tmp"),
            ("TMP", "/c/native/inv/tmp"),
            ("TEMP", "/c/native/inv/tmp"),
            ("CODEX_HOME", "/c/native/inv/codex-home"),
            ("LOOM_NATIVE_BOUNDARY", "inv-1"),
            ("HOME", "/Users/u"),
            ("CARGO_HOME", "/Users/u/.cargo"),
        ] {
            assert_eq!(value(name), Some(expected.into()), "{name}");
        }
        assert_eq!(value("OPENAI_API_KEY"), None);
        assert_eq!(value("SSH_AUTH_SOCK"), None);
        let path = value("PATH").expect("PATH");
        let first = std::env::split_paths(&path).next();
        assert_eq!(first.as_deref(), Some(Path::new("/opt/loom/bin")));
    }
}
