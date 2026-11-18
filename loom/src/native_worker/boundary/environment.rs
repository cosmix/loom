//! The boundary's environment, built from scratch, and the command the Cargo
//! scratch reads it from.
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;

use super::BOUNDARY_TMPDIR;

/// Where the launcher is mounted inside the boundary.
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
    // Workers compile: the toolchain proxies live in the Cargo home.
    path.extend(host_cargo_home(host).map(|cargo| cargo.join("bin")));
    path.extend(["/usr/local/bin", "/usr/bin", "/bin"].map(PathBuf::from));
    // An entry holding the separator cannot be expressed in PATH.
    let path = path
        .into_iter()
        .filter(|entry| !entry.as_os_str().as_encoded_bytes().contains(&b':'));
    let path = std::env::join_paths(path).unwrap_or_else(|_| "/usr/bin:/bin".into());
    let mut env: Vec<(OsString, OsString)> = vec![("PATH".into(), path)];
    if let Some(home) = lookup(host, "HOME") {
        env.push(("HOME".into(), home.to_os_string()));
    }
    let fixed = [
        ("LANG", "C.UTF-8"),
        ("TERM", "dumb"),
        ("TMPDIR", BOUNDARY_TMPDIR),
        ("TMP", BOUNDARY_TMPDIR),
        ("TEMP", BOUNDARY_TMPDIR),
        ("GIT_CONFIG_GLOBAL", "/dev/null"),
        ("LOOM_CODEX_FORWARD", "1"),
    ];
    env.extend(fixed.map(|(key, value)| (key.into(), value.into())));
    if launch {
        env.push(("LOOM_BIN".into(), LOOM_BIN.into()));
    }
    for key in ["RUSTUP_HOME", "CARGO_HOME"] {
        if let Some(value) = lookup(host, key) {
            env.push((key.into(), value.to_os_string()));
        }
    }
    env
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
